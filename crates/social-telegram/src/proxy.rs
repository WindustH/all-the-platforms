//! MTProto through a proxy.
//!
//! grammers dials SOCKS5 proxies itself. Most local proxies (and `--proxy`
//! values) are HTTP, so for those a small SOCKS5 server on 127.0.0.1 accepts
//! grammers' connections and carries each through an HTTP `CONNECT` tunnel.

use std::net::{Ipv4Addr, Ipv6Addr};

use social_core::{Error, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use url::Url;

/// The SOCKS5 URL grammers should use for `proxy` (as `Http::proxy` gives
/// it), starting the local bridge for HTTP proxies.
pub fn socks_url(proxy: Option<String>) -> Result<Option<String>> {
  let Some(raw) = proxy else {
    return Ok(None);
  };
  let raw = raw.trim();
  let raw = if raw.contains("://") {
    raw.to_owned()
  } else {
    format!("http://{raw}")
  };
  let url = Url::parse(&raw).map_err(|e| Error::input(format!("bad proxy {raw}: {e}")))?;
  match url.scheme() {
    "socks5" | "socks5h" => Ok(Some(raw.replacen("socks5h://", "socks5://", 1))),
    "http" | "https" => {
      let host = url.host_str().unwrap_or("127.0.0.1").to_owned();
      let port = url.port_or_known_default().unwrap_or(8080);
      bridge(format!("{host}:{port}")).map(Some)
    }
    other => Err(Error::input(format!(
      "unsupported proxy scheme `{other}` for Telegram (use http:// or socks5://)"
    ))),
  }
}

/// Listen on a free local port; each connection becomes a CONNECT tunnel
/// through `upstream`.
fn bridge(upstream: String) -> Result<String> {
  let std = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
  std.set_nonblocking(true)?;
  let port = std.local_addr()?.port();
  let listener = TcpListener::from_std(std)?;
  tokio::spawn(async move {
    while let Ok((client, _)) = listener.accept().await {
      let upstream = upstream.clone();
      tokio::spawn(async move {
        if let Err(e) = serve(client, &upstream).await {
          tracing::debug!("telegram proxy bridge: {e}");
        }
      });
    }
  });
  Ok(format!("socks5://127.0.0.1:{port}"))
}

/// One SOCKS5 CONNECT (no authentication) relayed over HTTP CONNECT.
async fn serve(mut client: TcpStream, upstream: &str) -> std::io::Result<()> {
  let mut head = [0u8; 2];
  client.read_exact(&mut head).await?;
  let mut methods = vec![0u8; head[1] as usize];
  client.read_exact(&mut methods).await?;
  client.write_all(&[5, 0]).await?;

  let mut req = [0u8; 4];
  client.read_exact(&mut req).await?;
  let host = match req[3] {
    1 => {
      let mut a = [0u8; 4];
      client.read_exact(&mut a).await?;
      Ipv4Addr::from(a).to_string()
    }
    4 => {
      let mut a = [0u8; 16];
      client.read_exact(&mut a).await?;
      format!("[{}]", Ipv6Addr::from(a))
    }
    3 => {
      let mut len = [0u8; 1];
      client.read_exact(&mut len).await?;
      let mut name = vec![0u8; len[0] as usize];
      client.read_exact(&mut name).await?;
      String::from_utf8_lossy(&name).into_owned()
    }
    _ => return Err(std::io::Error::other("unsupported SOCKS5 address type")),
  };
  let mut port = [0u8; 2];
  client.read_exact(&mut port).await?;
  let target = format!("{host}:{}", u16::from_be_bytes(port));

  let mut server = TcpStream::connect(upstream).await?;
  server
    .write_all(format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n").as_bytes())
    .await?;
  let mut answer = Vec::new();
  let mut byte = [0u8; 1];
  while !answer.ends_with(b"\r\n\r\n") {
    if server.read(&mut byte).await? == 0 || answer.len() > 8192 {
      return Err(std::io::Error::other("proxy closed during CONNECT"));
    }
    answer.push(byte[0]);
  }
  let status = String::from_utf8_lossy(&answer);
  let ok = status
    .split_whitespace()
    .nth(1)
    .is_some_and(|code| code.starts_with('2'));
  // SOCKS5 reply: succeeded, or general failure.
  let code = if ok { 0 } else { 1 };
  client.write_all(&[5, code, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
  if !ok {
    return Err(std::io::Error::other(format!(
      "proxy refused CONNECT {target}: {}",
      status.lines().next().unwrap_or_default()
    )));
  }
  tokio::io::copy_bidirectional(&mut client, &mut server).await?;
  Ok(())
}
