//! The linked device's WebSocket through a proxy.
//!
//! whatsapp-rust dials `web.whatsapp.com` directly; where that is blocked the
//! connection goes through the proxy named by `SOCIAL_PROXY`, `ALL_PROXY` or
//! `HTTPS_PROXY` (upper or lower case): an HTTP `CONNECT` tunnel for
//! `http://` proxies, SOCKS5 for `socks5://` (`socks5h://`). TLS and the
//! WebSocket handshake then run over the tunnel as without a proxy. Media
//! downloads (ureq) read the same variables themselves.

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use url::Url;
use whatsapp_rust::anyhow::{self, anyhow};
use whatsapp_rust::async_channel::Receiver;
use whatsapp_rust::async_trait;
use whatsapp_rust::transport::{
  TokioWebSocketTransportFactory, Transport, TransportEvent, TransportFactory,
  default_tls_connector, from_websocket,
};
use whatsapp_rust::wacore::net::{WHATSAPP_WEB_ORIGIN, WHATSAPP_WEB_WS_URL};

const HOST: &str = "web.whatsapp.com";
const PORT: u16 = 443;

/// Dials WhatsApp directly, or through the proxy from the environment.
pub struct Dialer {
  proxy: Option<Url>,
  direct: TokioWebSocketTransportFactory,
}

impl Dialer {
  pub fn from_env() -> Self {
    Self {
      proxy: proxy_from_env(),
      direct: TokioWebSocketTransportFactory::new(),
    }
  }
}

/// The proxy from the environment, if one is set.
fn proxy_from_env() -> Option<Url> {
  let var = |name: &str| {
    std::env::var(name)
      .or_else(|_| std::env::var(name.to_ascii_lowercase()))
      .ok()
      .filter(|v| !v.trim().is_empty())
  };
  let raw = var("SOCIAL_PROXY")
    .or_else(|| var("ALL_PROXY"))
    .or_else(|| var("HTTPS_PROXY"))?;
  let raw = if raw.contains("://") {
    raw
  } else {
    format!("http://{raw}")
  };
  let proxy = Url::parse(&raw).ok()?;
  matches!(proxy.scheme(), "http" | "https" | "socks5" | "socks5h").then_some(proxy)
}

async fn tunnel(proxy: &Url) -> anyhow::Result<TcpStream> {
  let host = proxy
    .host_str()
    .ok_or_else(|| anyhow!("proxy without a host"))?;
  let port = proxy.port_or_known_default().unwrap_or(1080);
  if proxy.scheme().starts_with("socks5") {
    let stream = if proxy.username().is_empty() {
      tokio_socks::tcp::Socks5Stream::connect((host, port), (HOST, PORT)).await?
    } else {
      tokio_socks::tcp::Socks5Stream::connect_with_password(
        (host, port),
        (HOST, PORT),
        proxy.username(),
        proxy.password().unwrap_or_default(),
      )
      .await?
    };
    return Ok(stream.into_inner());
  }
  let mut stream = TcpStream::connect((host, port)).await?;
  let mut request = format!("CONNECT {HOST}:{PORT} HTTP/1.1\r\nHost: {HOST}:{PORT}\r\n");
  if !proxy.username().is_empty() {
    use base64::Engine;
    let credentials = format!(
      "{}:{}",
      proxy.username(),
      proxy.password().unwrap_or_default()
    );
    let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
    request.push_str(&format!("Proxy-Authorization: Basic {encoded}\r\n"));
  }
  request.push_str("\r\n");
  stream.write_all(request.as_bytes()).await?;
  let mut head = Vec::new();
  let mut byte = [0u8; 1];
  while !head.ends_with(b"\r\n\r\n") {
    if stream.read(&mut byte).await? == 0 || head.len() > 8192 {
      return Err(anyhow!("the proxy closed the CONNECT tunnel"));
    }
    head.push(byte[0]);
  }
  let status = String::from_utf8_lossy(&head);
  let ok = status
    .lines()
    .next()
    .is_some_and(|l| l.split_whitespace().nth(1) == Some("200"));
  if !ok {
    return Err(anyhow!(
      "the proxy refused the tunnel: {}",
      status.lines().next().unwrap_or_default()
    ));
  }
  Ok(stream)
}

#[async_trait]
impl TransportFactory for Dialer {
  async fn create_transport(
    &self,
  ) -> anyhow::Result<(Arc<dyn Transport>, Receiver<TransportEvent>)> {
    let Some(proxy) = &self.proxy else {
      return self.direct.create_transport().await;
    };
    let stream = tunnel(proxy).await?;
    let tls = default_tls_connector().wrap(HOST, stream).await?;
    let uri: http::Uri = WHATSAPP_WEB_WS_URL.parse()?;
    let builder = tokio_websockets::ClientBuilder::from_uri(uri).add_header(
      http::header::ORIGIN,
      http::HeaderValue::from_static(WHATSAPP_WEB_ORIGIN),
    )?;
    let (ws, _) = builder.connect_on(tls).await?;
    Ok(from_websocket(ws))
  }
}
