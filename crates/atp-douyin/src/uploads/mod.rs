//! Files for posts, as the creator page's upload SDK (`lib-byted-uploader`)
//! sends them: temporary keys from `/web/api/media/upload/auth/v5/`, then
//! ByteDance's upload gateways signed with them (AWS Signature V4): images
//! through ImageX (`ApplyImageUpload`, TOS, `CommitImageUpload`), videos
//! through VOD (`ApplyUploadInner`, TOS, `CommitUploadInner` with `GetMeta`
//! and a first-frame `Snapshot`, the default cover), the bytes themselves
//! to TOS ([`tos`]); a video that stopped half-way continues on the next
//! run.

mod tos;

use std::path::Path;
use std::time::Duration;

use atp_bytedance::sigv4::{self, Keys, Request, Scope};
use atp_core::http::{Method, status_error};
use atp_core::text::encode_uri_component;
use atp_core::upload::{LocalFile, Parts, Resume};
use atp_core::{Error, ErrorCode, Result, Value, ValueExt, json};
use rand::Rng;

use self::tos::Node;
use crate::creator::{CREATOR, Creator};

const AUTH: &str = "/web/api/media/upload/auth/v5/";
const ENABLE: &str = "/web/api/media/video/enable/";
const VOD: &str = "https://vod.bytedanceapi.com/";
const IMAGEX: &str = "https://imagex.bytedanceapi.com/";
const REGION: &str = "cn-north-1";
const APP_ID: &str = "2906";
const SPACE: &str = "aweme";
const IMAGE_SERVICE: &str = "jm8ajry58r";
const RESUME_TTL: Duration = Duration::from_secs(2 * 3600);

/// An image on Douyin's storage.
#[derive(Debug, Clone)]
pub struct Image {
  pub uri: String,
  pub width: u64,
  pub height: u64,
}

/// A video on VOD, with the first frame as its default cover.
#[derive(Debug, Clone)]
pub struct Video {
  pub vid: String,
  pub poster: String,
}

/// Temporary keys of the upload gateways.
struct Sts {
  access_key: String,
  secret: String,
  token: String,
  /// Seconds the gateway's clock is ahead, when it is off by over a minute.
  gap: i64,
}

/// The upload gateways with one set of temporary keys, for `user` (the
/// account's uid, as the SDK's `userId`).
pub struct Uploader<'a> {
  creator: &'a Creator<'a>,
  keys: Sts,
  user: String,
}

impl<'a> Uploader<'a> {
  pub async fn new(creator: &'a Creator<'a>, user: &str) -> Result<Self> {
    Ok(Self {
      keys: sts(creator).await?,
      creator,
      user: user.to_owned(),
    })
  }

  /// Upload an image (ImageX).
  pub async fn image(&self, path: &Path) -> Result<Image> {
    let file = LocalFile::open(path).await?;
    let query = self.query(
      &["ApplyImageUpload", "2018-08-01"],
      ("ServiceId", IMAGE_SERVICE),
      true,
    );
    let result = self.gateway(IMAGEX, "imagex", query, None).await?;
    let address = result.at("UploadAddress");
    let node = node(address, address.str("UploadHosts.0"))?;
    let parts = Parts::new(&self.creator.api.ctx, &file, &node.store_uri);
    let mut state = Resume::new(node);
    self.send_file(&file, &parts, &mut state).await?;
    let body = json!({ "SessionKey": state.session.session_key }).to_string();
    let query = self.query(
      &["CommitImageUpload", "2018-08-01"],
      ("ServiceId", IMAGE_SERVICE),
      false,
    );
    let result = self
      .gateway(IMAGEX, "imagex", query, Some((body, "application/json")))
      .await?;
    parts.forget();
    image_of(&result, path).await
  }

  /// Upload a video (VOD); a run that stopped continues where it was.
  pub async fn video(&self, path: &Path) -> Result<Video> {
    let file = LocalFile::open(path).await?;
    let parts = Parts::new(&self.creator.api.ctx, &file, "douyin-vod");
    let mut state = match parts.resume::<Node>(RESUME_TTL) {
      Some(state) => state,
      None => Resume::new(self.apply_video(&file).await?),
    };
    parts.keep(&state);
    if let Err(e) = self.send_file(&file, &parts, &mut state).await {
      return Err(forget_unless_transient(&parts, e));
    }
    let video = self.commit_video(&state.session).await;
    parts.forget();
    let video = video?;
    // The page tells the creator platform the upload is done, and does not wait.
    let enable = self.creator.get(ENABLE).arg("video_id", &video.vid).send();
    if let Err(e) = enable.await {
      tracing::debug!("video/enable: {e}");
    }
    Ok(video)
  }

  async fn apply_video(&self, file: &LocalFile) -> Result<Node> {
    let mut query = pairs(&[
      ("Action", "ApplyUploadInner"),
      ("Version", "2020-11-19"),
      ("SpaceName", SPACE),
      ("FileType", "video"),
      ("IsInner", "1"),
    ]);
    query.push(("FileSize".into(), file.size.to_string()));
    self.identify(&mut query, true);
    let result = self.gateway(VOD, "vod", query, None).await?;
    let found = result.at("InnerUploadAddress.UploadNodes.0");
    node(found, found.str("UploadHost"))
  }

  async fn commit_video(&self, node: &Node) -> Result<Video> {
    let body = json!({
      "SessionKey": node.session_key,
      "Functions": [{ "name": "GetMeta" }, { "name": "Snapshot", "input": { "SnapshotTime": 0 } }],
    })
    .to_string();
    let query = self.query(
      &["CommitUploadInner", "2020-11-19"],
      ("SpaceName", SPACE),
      false,
    );
    let result = self
      .gateway(VOD, "vod", query, Some((body, "text/plain;charset=UTF-8")))
      .await?;
    let done = result.at("Results.0");
    let vid = done
      .str("Vid")
      .or_else(|| node.vid.clone())
      .ok_or_else(|| Error::upstream("Douyin's VOD committed the upload without a video id"))?;
    Ok(Video {
      vid,
      poster: done.str("PosterUri").unwrap_or_default(),
    })
  }

  /// `Action` and `Version`, the space or service, then what the SDK adds
  /// (`app_id`, `user_id`, and on `Apply` calls a random `s`).
  fn query(&self, action: &[&str; 2], space: (&str, &str), apply: bool) -> Vec<(String, String)> {
    let mut query = pairs(&[("Action", action[0]), ("Version", action[1]), space]);
    self.identify(&mut query, apply);
    query
  }

  fn identify(&self, query: &mut Vec<(String, String)>, apply: bool) {
    query.push(("app_id".into(), APP_ID.into()));
    query.push(("user_id".into(), self.user.clone()));
    if apply {
      query.push(("s".into(), random_s()));
    }
  }

  /// A signed call to an upload gateway; its `Result`.
  async fn gateway(
    &self,
    base: &str,
    service: &str,
    query: Vec<(String, String)>,
    body: Option<(String, &'static str)>,
  ) -> Result<Value> {
    let method = if body.is_some() {
      Method::POST
    } else {
      Method::GET
    };
    let headers = self.sign(
      &method,
      service,
      &query,
      body.as_ref().map(|(b, _)| b.as_str()),
    )?;
    let mut req = self
      .creator
      .api
      .ctx
      .http
      .request(method, format!("{base}?{}", url_query(&query)))
      .no_cookies()
      .no_throttle()
      .header("referer", format!("{CREATOR}/"))
      .header("origin", CREATOR);
    for (name, value) in &headers {
      req = req.header(name, value);
    }
    if let Some((body, content_type)) = body {
      req = req.bytes(body.into_bytes(), content_type);
    }
    let resp = req.send().await?;
    let v = resp
      .value()
      .map_err(|_| status_error(resp.status, &resp.text()))?;
    let error = v.at("ResponseMetadata.Error");
    if !error.is_null() {
      let action = query.first().map(|(_, a)| a.as_str()).unwrap_or_default();
      return Err(Error::upstream(format!(
        "Douyin's upload gateway refused {action}: {} {}",
        error.str("Code").unwrap_or_default(),
        error.str("Message").unwrap_or_default()
      )));
    }
    Ok(v.at("Result").clone())
  }

  /// The SigV4 headers of a gateway call, as the SDK signs them (on the
  /// gateway's clock), `authorization` last.
  fn sign(
    &self,
    method: &Method,
    service: &str,
    query: &[(String, String)],
    body: Option<&str>,
  ) -> Result<Vec<(String, String)>> {
    let now = jiff::Timestamp::now().as_second() + self.keys.gap;
    let now = jiff::Timestamp::from_second(now).map_err(|e| Error::internal(e.to_string()))?;
    let date = sigv4::amz_date(now);
    let hash = sigv4::sha256_hex(body.unwrap_or_default().as_bytes());
    let mut headers = vec![
      ("x-amz-date".to_owned(), date.clone()),
      ("x-amz-security-token".to_owned(), self.keys.token.clone()),
    ];
    if body.is_some() {
      headers.push(("x-amz-content-sha256".to_owned(), hash.clone()));
    }
    let request = Request {
      method: method.as_str(),
      path: "/",
      query,
      headers: &headers,
      payload_hash: &hash,
    };
    let keys = Keys {
      access_key: &self.keys.access_key,
      secret: &self.keys.secret,
    };
    let scope = Scope {
      region: REGION,
      service,
    };
    let authorization = sigv4::authorization(&request, &keys, &scope, &date);
    headers.push(("authorization".to_owned(), authorization));
    Ok(headers)
  }
}

/// The temporary keys the creator platform hands out for uploads.
async fn sts(creator: &Creator<'_>) -> Result<Sts> {
  let v = creator.get(AUTH).send().await?;
  let auth: Value = v
    .str("auth")
    .and_then(|a| serde_json::from_str(&a).ok())
    .ok_or_else(|| Error::upstream("Douyin handed out no upload keys"))?;
  let field = |key: &str| {
    auth
      .str(key)
      .filter(|s| !s.is_empty())
      .ok_or_else(|| Error::upstream(format!("Douyin's upload keys lack {key}")))
  };
  let gap = auth
    .str("CurrentTime")
    .and_then(|t| t.parse::<jiff::Timestamp>().ok())
    .map(|t| t.as_second() - jiff::Timestamp::now().as_second())
    .filter(|gap| gap.abs() > 60)
    .unwrap_or(0);
  Ok(Sts {
    access_key: field("AccessKeyID")?,
    secret: field("SecretAccessKey")?,
    token: field("SessionToken")?,
    gap,
  })
}

/// The node of an `Apply` answer (`UploadAddress` or `UploadNodes[0]`).
fn node(v: &Value, host: Option<String>) -> Result<Node> {
  let store = v.at("StoreInfos.0");
  let need = |value: Option<String>, what: &str| {
    value
      .filter(|s| !s.is_empty())
      .ok_or_else(|| Error::upstream(format!("Douyin's upload gateway gave no {what}")))
  };
  let header = v
    .at("UploadHeader")
    .as_object()
    .map(|m| {
      m.iter()
        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
        .collect()
    })
    .unwrap_or_default();
  Ok(Node {
    host: need(host, "upload host")?,
    store_uri: need(store.str("StoreUri"), "store uri")?,
    auth: need(store.str("Auth"), "upload credential")?,
    upload_id: store.str("UploadID").filter(|s| !s.is_empty()),
    session_key: need(v.str("SessionKey"), "session key")?,
    header,
    vid: v.str("Vid"),
  })
}

/// The committed image: `Results[0]` with `PluginResult[0]`; its size
/// read from the file when ImageX leaves it out.
async fn image_of(result: &Value, path: &Path) -> Result<Image> {
  let uri = result
    .first_str(&["PluginResult.0.ImageUri", "Results.0.Uri"])
    .ok_or_else(|| Error::upstream("Douyin's ImageX committed the image without a uri"))?;
  let mut width = result.u64("PluginResult.0.ImageWidth").unwrap_or(0);
  let mut height = result.u64("PluginResult.0.ImageHeight").unwrap_or(0);
  if width == 0 || height == 0 {
    let (w, h) = atp_core::file::Image::read(path).await?.size();
    (width, height) = (w as u64, h as u64);
  }
  Ok(Image { uri, width, height })
}

/// Keep the state of an upload only when another run may continue it.
fn forget_unless_transient(parts: &Parts<'_>, e: Error) -> Error {
  if matches!(e.code, ErrorCode::NetworkError | ErrorCode::RateLimited) {
    return e.with_hint("run the command again to continue the upload");
  }
  parts.forget();
  e
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
  list
    .iter()
    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
    .collect()
}

/// The query as the SDK writes it: in order, `encodeURIComponent`.
fn url_query(query: &[(String, String)]) -> String {
  query
    .iter()
    .map(|(k, v)| format!("{k}={}", encode_uri_component(v)))
    .collect::<Vec<_>>()
    .join("&")
}

/// `Math.random().toString(36).substr(2)`.
fn random_s() -> String {
  const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
  let mut rng = rand::rng();
  (0..11)
    .map(|_| char::from(DIGITS[rng.random_range(0..DIGITS.len())]))
    .collect()
}
