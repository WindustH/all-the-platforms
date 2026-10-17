//! TOS, the object storage behind the gateways: files up to 3 MB in one
//! request, larger ones in parts (`phase=transfer`, then `phase=finish`
//! with each part's CRC-32), the parts retried and kept for a later run.

use std::time::Duration;

use atp_core::http::status_error;
use atp_core::text::encode_uri_component;
use atp_core::upload::{LocalFile, Part, Parts, Resume};
use atp_core::{Error, Result, Value, ValueExt};
use rand::Rng;
use serde::{Deserialize, Serialize};

use super::Uploader;
use crate::creator::CREATOR;

const MB: u64 = 1 << 20;
/// Files up to this size go in one request.
const DIRECT_MAX: u64 = 3 * MB;
/// Parts through the storage gateway are at least this big.
const GATEWAY_PART_MIN: u64 = 5 * MB;
/// Headers of an `Apply` answer that mark the storage gateway.
const GATEWAY_HEADERS: &[&str] = &["x-storage-mode", "x-upload-storage-mode"];

/// Where TOS takes one file, from an `Apply` answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Node {
  pub host: String,
  pub store_uri: String,
  pub auth: String,
  pub upload_id: Option<String>,
  pub session_key: String,
  pub header: Vec<(String, String)>,
  pub vid: Option<String>,
}

impl Node {
  fn gateway(&self) -> bool {
    self
      .header
      .iter()
      .any(|(k, _)| GATEWAY_HEADERS.contains(&k.to_ascii_lowercase().as_str()))
  }

  fn url(&self, query: &str) -> String {
    let base = format!("https://{}/upload/v1/{}", self.host, self.store_uri);
    if query.is_empty() {
      base
    } else {
      format!("{base}?{query}")
    }
  }
}

impl Uploader<'_> {
  /// Send a file to its node: whole when small, else part by part.
  pub(super) async fn send_file(
    &self,
    file: &LocalFile,
    parts: &Parts<'_>,
    state: &mut Resume<Node>,
  ) -> Result<()> {
    if file.size <= DIRECT_MAX {
      let data = file.part(1, file.size.max(1)).await?;
      let crc = crc32(&data);
      let node = &state.session;
      return self
        .tos(
          node,
          node.url(""),
          Some(&crc),
          Some((data, "application/octet-stream")),
        )
        .await;
    }
    if state.session.upload_id.is_none() {
      state.session.upload_id = Some(self.upload_id(&state.session).await?);
      parts.keep(state);
    }
    let node = state.session.clone();
    let upload_id = node.upload_id.clone().unwrap_or_default();
    let size = part_size(file.size, node.gateway());
    parts
      .send(state, size, async |part| {
        self.transfer(&node, &upload_id, size, part).await
      })
      .await?;
    let body = state
      .sent
      .iter()
      .enumerate()
      .map(|(i, crc)| format!("{}:{crc}", i + 1))
      .collect::<Vec<_>>()
      .join(",");
    let url = node.url(&format!(
      "uploadmode=part&phase=finish&uploadid={upload_id}"
    ));
    self
      .tos(
        &node,
        url,
        None,
        Some((body.into_bytes(), "text/plain;charset=UTF-8")),
      )
      .await
  }

  /// One part; what TOS knows it by is its CRC-32.
  async fn transfer(&self, node: &Node, upload_id: &str, size: u64, part: &Part) -> Result<String> {
    let crc = crc32(&part.data);
    let offset = u64::from(part.number - 1) * size;
    let query = format!(
      "uploadid={upload_id}&part_number={}&phase=transfer&part_offset={offset}",
      part.number
    );
    let body = Some((part.data.clone(), "application/octet-stream"));
    self.tos(node, node.url(&query), Some(&crc), body).await?;
    Ok(crc)
  }

  /// The id of a parts upload: the one `Apply` named, else the gateway's
  /// (`phase=init`), else a fresh UUID, as the SDK does.
  async fn upload_id(&self, node: &Node) -> Result<String> {
    if !node.gateway() {
      return Ok(uuid());
    }
    let url = node.url("uploadmode=part&phase=init");
    let v = self.tos_value(node, url, None, None).await?;
    v.str("data.uploadid")
      .ok_or_else(|| Error::upstream("Douyin's storage gave no upload id"))
  }

  async fn tos(
    &self,
    node: &Node,
    url: String,
    crc: Option<&str>,
    body: Option<(Vec<u8>, &str)>,
  ) -> Result<()> {
    self.tos_value(node, url, crc, body).await.map(|_| ())
  }

  /// A request to TOS with the node's credentials; its answer must say
  /// `code` 2000.
  async fn tos_value(
    &self,
    node: &Node,
    url: String,
    crc: Option<&str>,
    body: Option<(Vec<u8>, &str)>,
  ) -> Result<Value> {
    let mut req = self
      .creator
      .api
      .ctx
      .http
      .post(url)
      .no_cookies()
      .no_throttle()
      .timeout(Duration::from_secs(300))
      .header("authorization", &node.auth)
      .header("x-storage-u", encode_uri_component(&self.user))
      .header("referer", format!("{CREATOR}/"))
      .header("origin", CREATOR);
    if let Some(crc) = crc {
      req = req
        .header("content-crc32", crc)
        .header("content-disposition", "attachment; filename=\"undefined\"");
    }
    for (name, value) in &node.header {
      req = req.header(name, value);
    }
    if let Some((data, content_type)) = body {
      req = req.bytes(data, content_type);
    }
    let resp = req.send().await?;
    if !resp.status.is_success() {
      return Err(status_error(resp.status, &resp.text()));
    }
    let v = resp.value()?;
    if v.i64("code") != Some(2000) {
      return Err(Error::upstream(format!(
        "Douyin's storage refused the upload: {} ({})",
        v.str("message").unwrap_or_default(),
        v.i64("code").unwrap_or_default()
      )));
    }
    Ok(v)
  }
}

/// Part size by file size, as the SDK picks it (3, 5 or 10 MB).
fn part_size(size: u64, gateway: bool) -> u64 {
  let slice = if size >= 500 * MB {
    10 * MB
  } else if size >= 100 * MB {
    5 * MB
  } else {
    3 * MB
  };
  if gateway {
    slice.max(GATEWAY_PART_MIN)
  } else {
    slice
  }
}

fn crc32(data: &[u8]) -> String {
  format!("{:08x}", crc32fast::hash(data))
}

/// A random UUID (version 4).
fn uuid() -> String {
  let mut bytes: [u8; 16] = rand::rng().random();
  bytes[6] = (bytes[6] & 0x0F) | 0x40;
  bytes[8] = (bytes[8] & 0x3F) | 0x80;
  let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
  format!(
    "{}-{}-{}-{}-{}",
    &hex[..8],
    &hex[8..12],
    &hex[12..16],
    &hex[16..20],
    &hex[20..]
  )
}
