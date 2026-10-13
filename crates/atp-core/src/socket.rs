//! Websocket connections for live events (chat gateways, streaming APIs),
//! through the same fingerprinted client as every other request.

use serde_json::Value;
use wreq::header::OrigHeaderMap;
use wreq::ws::message::Message;

use crate::error::{Error, Result};
use crate::http::Http;

/// One frame with data; pings, pongs and close frames are handled here.
pub enum Frame {
  Text(String),
  Binary(Vec<u8>),
}

pub struct Socket {
  inner: wreq::ws::WebSocket,
}

impl Http {
  /// Open a websocket. The session cookies go along when `cookies` is set
  /// (gateways that authenticate by cookie); `headers` are added as given.
  ///
  /// Header names keep a browser's spelling and order: gateways behind
  /// Cloudflare (Discord's) refuse handshakes with lowercase names.
  pub async fn websocket(
    &self,
    url: &str,
    headers: &[(&str, &str)],
    cookies: bool,
  ) -> Result<Socket> {
    let mut rb = self.client().websocket(url);
    let mut names = OrigHeaderMap::new();
    for name in [
      "Host",
      "Connection",
      "Pragma",
      "Cache-Control",
      "User-Agent",
      "Upgrade",
      "Origin",
    ] {
      names.insert(name);
    }
    for (k, v) in headers {
      rb = rb.header(*k, *v);
      names.insert(k.to_string());
    }
    for name in [
      "Sec-WebSocket-Version",
      "Accept-Language",
      "Sec-WebSocket-Key",
      "Cookie",
    ] {
      names.insert(name);
    }
    rb = rb.orig_headers(names);
    if cookies {
      let cookie = self.cookie_header();
      if !cookie.is_empty() {
        rb = rb.header("cookie", cookie);
      }
    }
    let inner = rb.send().await?.into_websocket().await?;
    Ok(Socket { inner })
  }
}

impl Socket {
  pub async fn send_text(&mut self, text: impl Into<String>) -> Result<()> {
    let text: String = text.into();
    Ok(self.inner.send(Message::text(text)).await?)
  }

  pub async fn send_json(&mut self, value: &Value) -> Result<()> {
    self.send_text(value.to_string()).await
  }

  pub async fn send_binary(&mut self, data: Vec<u8>) -> Result<()> {
    Ok(self.inner.send(Message::binary(data)).await?)
  }

  /// The next text or binary frame; `None` once the server closed.
  pub async fn next(&mut self) -> Result<Option<Frame>> {
    while let Some(msg) = self.inner.recv().await {
      match msg? {
        Message::Text(t) => return Ok(Some(Frame::Text(t.to_string()))),
        Message::Binary(b) => return Ok(Some(Frame::Binary(b.to_vec()))),
        Message::Close(_) => return Ok(None),
        Message::Ping(_) | Message::Pong(_) => {}
      }
    }
    Ok(None)
  }

  /// The next frame parsed as JSON (binary frames as UTF-8 JSON too).
  pub async fn next_json(&mut self) -> Result<Option<Value>> {
    let text = match self.next().await? {
      None => return Ok(None),
      Some(Frame::Text(t)) => t,
      Some(Frame::Binary(b)) => {
        String::from_utf8(b).map_err(|_| Error::upstream("binary websocket frame is not text"))?
      }
    };
    Ok(Some(serde_json::from_str(&text)?))
  }
}
