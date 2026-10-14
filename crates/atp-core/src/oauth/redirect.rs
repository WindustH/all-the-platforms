//! The local end of a login that comes back through the browser: a
//! listener on the loopback interface takes what the authorization server
//! sends to the redirect address, in the query (an authorization code) or
//! after the `#` (an implicit grant's token, which browsers keep to
//! themselves; a small page sends it on as a query).

use std::collections::BTreeMap;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::error::{Error, Result};

/// Where the page of an implicit grant sends the part after `#`.
const FRAGMENT_PATH: &str = "/fragment";

/// The page that hands the part after `#` on to [`FRAGMENT_PATH`].
const FRAGMENT_PAGE: &str = "<!doctype html><meta charset=utf-8><title>ATP</title>\
<p>Handing the answer to ATP…</p><script>\
location.replace('/fragment?' + (location.hash.slice(1) || 'error=nothing_returned'));\
</script>";

/// Where the redirect carries its parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
  /// `?code=…&state=…` (authorization code).
  Query,
  /// `#access_token=…&state=…` (implicit grant).
  Fragment,
}

/// Accept connections on `listener` until the browser comes back to `path`
/// and return the parameters it brought, once their `state` matches.
/// `site` names the server in messages ("Spotify did not grant access").
pub async fn redirect(
  listener: &TcpListener,
  path: &str,
  state: &str,
  via: Via,
  site: &str,
) -> Result<BTreeMap<String, String>> {
  loop {
    let (mut stream, _) = listener.accept().await?;
    let Some(params) = request(&mut stream, path, via).await else {
      continue;
    };
    let result = check(params, state, site);
    let text = match &result {
      Ok(_) => {
        format!("ATP has {site}'s answer. You can close this tab and return to the terminal.")
      }
      Err(_) => format!("The {site} login did not complete; see the terminal."),
    };
    reply(&mut stream, "200 OK", "text/plain", &text).await;
    return result;
  }
}

/// Read one request: the redirect's parameters, or `None` after answering
/// anything else (other paths, the page that forwards a fragment).
async fn request(stream: &mut TcpStream, path: &str, via: Via) -> Option<BTreeMap<String, String>> {
  let mut buf = vec![0u8; 8192];
  let n = stream.read(&mut buf).await.ok()?;
  let request = String::from_utf8_lossy(&buf[..n]);
  let target = request.split_whitespace().nth(1)?;
  let url = url::Url::parse(&format!("http://127.0.0.1{target}")).ok()?;
  let params: BTreeMap<String, String> = url.query_pairs().into_owned().collect();
  let arrived = match via {
    Via::Query => url.path() == path,
    // Errors may come in the query, before any page runs.
    Via::Fragment => url.path() == FRAGMENT_PATH || (url.path() == path && !params.is_empty()),
  };
  if arrived {
    return Some(params);
  }
  if via == Via::Fragment && url.path() == path {
    reply(stream, "200 OK", "text/html", FRAGMENT_PAGE).await;
  } else {
    reply(stream, "404 Not Found", "text/plain", "Not found.").await;
  }
  None
}

fn check(
  params: BTreeMap<String, String>,
  state: &str,
  site: &str,
) -> Result<BTreeMap<String, String>> {
  if params.get("state").map(String::as_str) != Some(state) {
    return Err(Error::auth(
      "the browser came back from another login attempt; run login again",
    ));
  }
  match params.get("error") {
    Some(error) => Err(Error::auth(format!("{site} did not grant access: {error}"))),
    None => Ok(params),
  }
}

async fn reply(stream: &mut TcpStream, status: &str, kind: &str, text: &str) {
  let page = format!(
    "HTTP/1.1 {status}\r\ncontent-type: {kind}; charset=utf-8\r\ncontent-length: {}\r\n\
     connection: close\r\n\r\n{text}",
    text.len()
  );
  let _ = stream.write_all(page.as_bytes()).await;
  let _ = stream.shutdown().await;
}
