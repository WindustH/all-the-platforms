//! The few Chrome DevTools Protocol calls a human check needs: put the
//! session cookies in, open a page, read the cookies back.

use serde_json::{Value, json};

use crate::browser::{Stored, pick};
use crate::error::{Error, Result};
use crate::http::Cookies;
use crate::json::ValueExt;
use crate::socket::Socket;

pub struct Cdp {
  socket: Socket,
  next: u64,
}

impl Cdp {
  pub async fn connect(endpoint: &str) -> Result<Self> {
    Ok(Self {
      socket: Socket::local(endpoint).await?,
      next: 0,
    })
  }

  /// One command on the browser target; events in between are skipped.
  pub async fn call(&mut self, method: &str, params: Value) -> Result<Value> {
    self.next += 1;
    let id = self.next;
    let message = json!({"id": id, "method": method, "params": params});
    self.socket.send_json(&message).await?;
    while let Some(answer) = self.socket.next_json_lenient().await? {
      if answer.u64("id") != Some(id) {
        continue;
      }
      if let Some(error) = answer.get("error") {
        return Err(Error::internal(format!("{method}: {error}")));
      }
      return Ok(answer.at("result").clone());
    }
    Err(Error::internal(
      "the browser closed its DevTools connection",
    ))
  }

  /// Set `cookies` on every domain in `domains` and its subdomains.
  pub async fn set_cookies(&mut self, cookies: &Cookies, domains: &[&str]) -> Result<()> {
    let list: Vec<Value> = domains
      .iter()
      .flat_map(|d| cookies.iter().map(move |(k, v)| cookie(k, v, d)))
      .collect();
    if !list.is_empty() {
      self
        .call("Storage.setCookies", json!({"cookies": list}))
        .await?;
    }
    Ok(())
  }

  /// The browser's cookies on `domains`, one value per name.
  pub async fn cookies(&mut self, domains: &[&str]) -> Result<Cookies> {
    let answer = self.call("Storage.getCookies", json!({})).await?;
    let stored = answer.list("cookies").iter().filter_map(|c| {
      Some(Stored {
        name: c.str("name")?,
        value: c.str("value")?,
        domain: c.str("domain")?,
        expires: c
          .at("expires")
          .as_f64()
          .filter(|e| *e > 0.0)
          .map(|e| e as u64),
      })
    });
    Ok(pick(stored, domains))
  }

  /// Open `url` in a new tab and close the blank one the browser started with.
  pub async fn open(&mut self, url: &str) -> Result<()> {
    let targets = self.call("Target.getTargets", json!({})).await?;
    self
      .call("Target.createTarget", json!({"url": url}))
      .await?;
    for blank in targets
      .list("targetInfos")
      .iter()
      .filter(|t| t.str("type").as_deref() == Some("page"))
      .filter_map(|t| t.str("targetId"))
    {
      let _ = self
        .call("Target.closeTarget", json!({"targetId": blank}))
        .await;
    }
    Ok(())
  }

  pub async fn close(&mut self) {
    let _ = self.call("Browser.close", json!({})).await;
  }
}

/// A cookie for `domain` and its subdomains; `__Host-` cookies stay on the
/// host itself, as their prefix demands.
fn cookie(name: &str, value: &str, domain: &str) -> Value {
  if name.starts_with("__Host-") {
    return json!({
      "name": name, "value": value, "url": format!("https://{domain}/"),
      "path": "/", "secure": true,
    });
  }
  json!({
    "name": name, "value": value, "domain": format!(".{domain}"), "path": "/",
    "secure": name.starts_with("__Secure-"),
  })
}
