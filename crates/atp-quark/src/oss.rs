//! The Aliyun OSS side of an upload. Quark signs each OSS request itself:
//! atp sends the string to sign (`auth_meta`) to `/file/upload/auth` and
//! puts the `auth_key` it answers into `Authorization`. Parts are PUT in
//! order; the commit lists their ETags and carries Quark's callback.
//!
//! From the second part on, each part also states where SHA-1 stands after
//! the parts before it (`X-Oss-Hash-Ctx`, OpenSSL's state as JSON in
//! base64), as the Quark clients that send it do. Clients that leave it out
//! exist too; one real multi-part upload settles whether OSS insists.

use std::time::Duration;

use atp_core::http::{Method, Req, status_error};
use atp_core::upload::{Part, Sha1Midway};
use atp_core::{Error, Result, ValueExt, json};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use md5::{Digest, Md5};

use crate::api::{Api, REFERER};
use crate::uploads::Session;

/// The browser SDK the web drive uploads with.
const OSS_UA: &str = "aliyun-sdk-js/6.6.1 Chrome 98.0.4758.80 on Windows 10 64-bit";
const PART_TIMEOUT: Duration = Duration::from_secs(300);

/// PUT one part; its ETag.
pub async fn put(api: &Api, session: &Session, part: &Part) -> Result<String> {
  let date = now();
  let context = part
    .sha1
    .filter(|_| part.number > 1)
    .map(|m| hash_context(&m));
  let context_line = context
    .as_deref()
    .map(|c| format!("x-oss-hash-ctx:{c}\n"))
    .unwrap_or_default();
  let target = format!(
    "{}?partNumber={}&uploadId={}",
    session.obj_key, part.number, session.upload_id
  );
  let meta = format!(
    "PUT\n\n{}\n{date}\nx-oss-date:{date}\n{context_line}x-oss-user-agent:{OSS_UA}\n/{}/{target}",
    session.mime, session.bucket
  );
  let key = auth(api, session, &meta).await?;
  let mut req =
    oss(api, Method::PUT, session, &target, &key, &date).bytes(part.data.clone(), &session.mime);
  if let Some(context) = &context {
    req = req.header("x-oss-hash-ctx", context);
  }
  let resp = req.send().await?;
  if !resp.status.is_success() {
    return Err(refused(resp.status, &resp.text(), part.number));
  }
  resp
    .header("etag")
    .map(str::to_owned)
    .ok_or_else(|| Error::upstream(format!("OSS took part {} without an ETag", part.number)))
}

/// Tell OSS the parts are complete; Quark's callback runs on its side.
pub async fn commit(api: &Api, session: &Session, etags: &[String]) -> Result<()> {
  let mut xml =
    String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CompleteMultipartUpload>\n");
  for (i, etag) in etags.iter().enumerate() {
    xml.push_str(&format!(
      "<Part>\n<PartNumber>{}</PartNumber>\n<ETag>{etag}</ETag>\n</Part>\n",
      i + 1
    ));
  }
  xml.push_str("</CompleteMultipartUpload>");
  let content_md5 = STANDARD.encode(Md5::digest(xml.as_bytes()));
  let callback = json!({
    "callbackUrl": session.callback.str("callbackUrl").unwrap_or_default(),
    "callbackBody": session.callback.str("callbackBody").unwrap_or_default(),
  });
  let callback = STANDARD.encode(callback.to_string());
  let date = now();
  let target = format!("{}?uploadId={}", session.obj_key, session.upload_id);
  let meta = format!(
    "POST\n{content_md5}\napplication/xml\n{date}\nx-oss-callback:{callback}\nx-oss-date:{date}\nx-oss-user-agent:{OSS_UA}\n/{}/{target}",
    session.bucket
  );
  let key = auth(api, session, &meta).await?;
  let resp = oss(api, Method::POST, session, &target, &key, &date)
    .header("content-md5", &content_md5)
    .header("x-oss-callback", &callback)
    .bytes(xml.into_bytes(), "application/xml")
    .send()
    .await?;
  if !resp.status.is_success() {
    return Err(refused(resp.status, &resp.text(), 0));
  }
  Ok(())
}

/// Quark's signature for one OSS request.
async fn auth(api: &Api, session: &Session, meta: &str) -> Result<String> {
  let body = json!({
    "auth_info": session.auth_info,
    "auth_meta": meta,
    "task_id": session.task_id,
  });
  let v = api.post("/file/upload/auth", &body).await?;
  v.str("data.auth_key")
    .ok_or_else(|| Error::upstream("Quark gave no signature for the upload"))
}

/// A signed request to the bucket, without the session cookies.
fn oss<'a>(
  api: &'a Api,
  method: Method,
  session: &Session,
  target: &str,
  key: &str,
  date: &str,
) -> Req<'a> {
  let url = format!("https://{}.{}/{target}", session.bucket, session.host);
  api
    .ctx
    .http
    .request(method, url)
    .no_cookies()
    .no_throttle()
    .retries(0)
    .timeout(PART_TIMEOUT)
    .header("authorization", key)
    .header("x-oss-date", date)
    .header("x-oss-user-agent", OSS_UA)
    .header("referer", REFERER)
}

/// `X-Oss-Hash-Ctx`: SHA-1 after the parts so far, as OpenSSL keeps it.
fn hash_context(midway: &Sha1Midway) -> String {
  let [h0, h1, h2, h3, h4] = midway.words;
  let (low, high) = midway.bits();
  let context = json!({
    "hash_type": "sha1",
    "h0": h0.to_string(),
    "h1": h1.to_string(),
    "h2": h2.to_string(),
    "h3": h3.to_string(),
    "h4": h4.to_string(),
    "Nl": low.to_string(),
    "Nh": high.to_string(),
    "data": "",
    "num": "0",
  });
  STANDARD.encode(context.to_string())
}

/// What OSS said when it refused a part (0: the commit).
fn refused(status: atp_core::http::StatusCode, body: &str, part: u32) -> Error {
  let what = if part == 0 {
    "the upload's commit".to_owned()
  } else {
    format!("part {part}")
  };
  let e = status_error(status, body);
  Error::new(e.code, format!("OSS refused {what}: {}", e.message))
}

/// `Tue, 14 Oct 2026 08:00:00 GMT`.
fn now() -> String {
  jiff::Timestamp::now()
    .strftime("%a, %d %b %Y %H:%M:%S GMT")
    .to_string()
}
