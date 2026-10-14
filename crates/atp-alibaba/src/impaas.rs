//! DingTalk IMPaaS, the service behind both sites' web chat: one websocket
//! (`wss://wss-<site>.dingtalk.com/`) carrying JSON frames with an `lwp`
//! path, `headers` and a `body`. A session registers (`/reg`) with an access
//! token from the site's mtop gateway, acknowledges every frame the server
//! sends, beats a heart every 15 s and pairs requests with their answers by
//! `headers.mid`. Pushes carry new messages in
//! `body.syncPushPackage.data[].data`: base64 MessagePack, or base64 JSON for
//! state changes, which are skipped.

use std::collections::VecDeque;
use std::time::Duration;

use atp_core::http::USER_AGENT;
use atp_core::socket::{Frame, Socket};
use atp_core::{Ctx, Error, Result, Value, ValueExt, credentials, json};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use rand::Rng;
use tokio::time::Instant;

use crate::msgpack;

/// One site's chat service.
#[derive(Debug)]
pub struct Profile {
  pub url: &'static str,
  pub origin: &'static str,
  pub app_key: &'static str,
  /// The `ua` the site's web client registers with.
  pub ua: &'static str,
  /// Suffix of user and chat ids: `<id>@<domain>`.
  pub domain: &'static str,
  /// A client-side id for a message about to be sent.
  pub uuid: fn() -> String,
  /// A new device id for the account `unb`.
  pub device: fn(&str) -> String,
}

pub static GOOFISH: Profile = Profile {
  url: "wss://wss-goofish.dingtalk.com/",
  origin: "https://www.goofish.com",
  app_key: "444e9908a51d1cb236a27862abc769c9",
  ua: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) \
       Chrome/133.0.0.0 Safari/537.36 DingTalk(2.1.5) OS(Windows/10) Browser(Chrome/133.0.0.0) \
       DingWeb/2.1.5 IMPaaS DingWeb/2.1.5",
  domain: "goofish",
  uuid: || format!("-{}1", now_ms()),
  device: |unb| format!("{}-{unb}", uuid_like()),
};

pub static CNTAOBAO: Profile = Profile {
  url: "wss://wss-cntaobao.dingtalk.com/",
  origin: "https://www.cntaobao.com",
  app_key: "3ce2dacdc7c0c43ad7bc7f9bc7d7a1b8",
  ua: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) \
       Chrome/146.0.0.0 Safari/537.36 DingTalk(2.1.5) OS(Windows/10) Browser(Chrome/146.0.0.0) \
       DingWeb/2.1.5 IMPaaS DingWeb/2.1.5",
  domain: "cntaobao",
  uuid: || format!("_s_{}_1", now_ms()),
  device: |_| format!("{}-{}", uuid_like().replace('-', ""), now_ms()),
};

/// Session extras: the device id, and the account it was made for.
const DEVICE_KEY: &str = "im_device";
const DEVICE_USER_KEY: &str = "im_device_user";
const HEARTBEAT: Duration = Duration::from_secs(15);
/// How long to wait for the server's `/s/vulcan` after registering.
const READY: Duration = Duration::from_secs(10);
const ANSWER: Duration = Duration::from_secs(20);

pub fn now_ms() -> i64 {
  jiff::Timestamp::now().as_millisecond()
}

/// A frame id: a random number, the time and ` 0`.
pub fn mid() -> String {
  format!("{}{} 0", rand::rng().random_range(0..1000), now_ms())
}

/// 36 characters shaped like a UUID v4, as the web clients make device ids.
fn uuid_like() -> String {
  const CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
  let mut rng = rand::rng();
  (0..36)
    .map(|i| match i {
      8 | 13 | 18 | 23 => '-',
      14 => '4',
      19 => char::from(CHARS[(rng.random_range(0..16) & 3) | 8]),
      _ => char::from(CHARS[rng.random_range(0..16)]),
    })
    .collect()
}

/// The device id of this account, made once and kept with the session.
pub fn device(ctx: &Ctx, profile: &Profile, unb: &str) -> String {
  if ctx.extra(DEVICE_USER_KEY).as_deref() == Some(unb)
    && let Some(device) = ctx.extra(DEVICE_KEY)
  {
    return device;
  }
  let device = (profile.device)(unb);
  credentials::save(ctx, &[(DEVICE_KEY, &device), (DEVICE_USER_KEY, unb)]);
  device
}

/// `<id>@<domain>`.
pub fn qualified(profile: &Profile, id: &str) -> String {
  format!("{}@{}", plain(id), profile.domain)
}

/// An id without its `@<domain>` suffix.
pub fn plain(id: &str) -> &str {
  id.split('@').next().unwrap_or(id)
}

/// What [`Im::next_frame`] got.
enum Next {
  Frame(Value),
  Timeout,
  Closed,
}

/// A registered chat connection.
pub struct Im {
  socket: Socket,
  pub profile: &'static Profile,
  pushes: VecDeque<Value>,
  beat: Instant,
}

impl Im {
  /// Connect and register with the IM access `token` and `device` id.
  pub async fn open(
    ctx: &Ctx,
    profile: &'static Profile,
    token: &str,
    device: &str,
  ) -> Result<Self> {
    let headers = [
      ("User-Agent", USER_AGENT),
      ("Origin", profile.origin),
      ("Pragma", "no-cache"),
      ("Cache-Control", "no-cache"),
      ("Accept-Language", "zh-CN,zh;q=0.9"),
    ];
    let socket = ctx.http.websocket(profile.url, &headers, true).await?;
    let mut im = Self {
      socket,
      profile,
      pushes: VecDeque::new(),
      beat: Instant::now() + HEARTBEAT,
    };
    im.register(token, device).await?;
    Ok(im)
  }

  async fn register(&mut self, token: &str, device: &str) -> Result<()> {
    let reg = mid();
    let headers = json!({
      "cache-header": "app-key token ua wv",
      "app-key": self.profile.app_key,
      "token": token,
      "ua": self.profile.ua,
      "dt": "j",
      "wv": "im:3,au:3,sy:6",
      "sync": "0,0;0;0;",
      "did": device,
      "mid": reg,
    });
    self
      .socket
      .send_json(&json!({"lwp": "/reg", "headers": headers}))
      .await?;
    let now = now_ms();
    let sync = json!([{
      "pipeline": "sync", "tooLong2Tag": "PNM,1", "channel": "sync", "topic": "sync",
      "highPts": 0, "pts": now * 1000, "seq": 0, "timestamp": now,
    }]);
    let ack_diff = json!({"lwp": "/r/SyncStatus/ackDiff", "headers": {"mid": mid()}, "body": sync});
    self.socket.send_json(&ack_diff).await?;
    let until = Instant::now() + READY;
    loop {
      let frame = match self.next_frame(until).await? {
        Next::Frame(frame) => frame,
        // Some sessions never say they are ready; requests work all the same.
        Next::Timeout => return Ok(()),
        Next::Closed => return Err(closed()),
      };
      if frame.str("lwp").as_deref() == Some("/s/vulcan") {
        return Ok(());
      }
      if frame.str("headers.mid").as_deref() == Some(reg.as_str()) {
        status(&frame, "/reg")?;
      }
    }
  }

  /// Send a request and wait for the `body` of its answer.
  pub async fn request(&mut self, lwp: &str, body: Value) -> Result<Value> {
    self.beat_if_due().await?;
    let mid = mid();
    let frame = json!({"lwp": lwp, "headers": {"mid": mid}, "body": body});
    self.socket.send_json(&frame).await?;
    let until = Instant::now() + ANSWER;
    loop {
      let mut frame = match self.next_frame(until).await? {
        Next::Frame(frame) => frame,
        Next::Timeout => {
          return Err(Error::network(format!(
            "the chat server did not answer {lwp}"
          )));
        }
        Next::Closed => return Err(closed()),
      };
      if frame.str("headers.mid").as_deref() == Some(mid.as_str()) && frame.get("code").is_some() {
        status(&frame, lwp)?;
        return Ok(frame.get_mut("body").map(Value::take).unwrap_or_default());
      }
    }
  }

  /// Messages pushed within `wait` (decoded MessagePack), beating the heart
  /// meanwhile; `None` once the server closed the connection.
  pub async fn pushes(&mut self, wait: Duration) -> Result<Option<Vec<Value>>> {
    let until = Instant::now() + wait;
    loop {
      if !self.pushes.is_empty() {
        return Ok(Some(self.pushes.drain(..).collect()));
      }
      if Instant::now() >= until {
        return Ok(Some(Vec::new()));
      }
      self.beat_if_due().await?;
      if let Next::Closed = self.next_frame(until.min(self.beat)).await? {
        return Ok(None);
      }
    }
  }

  async fn beat_if_due(&mut self) -> Result<()> {
    if Instant::now() >= self.beat {
      let beat = json!({"lwp": "/!", "headers": {"mid": mid()}});
      self.socket.send_json(&beat).await?;
      self.beat = Instant::now() + HEARTBEAT;
    }
    Ok(())
  }

  /// The next frame before `until`: acknowledged, its pushes queued.
  async fn next_frame(&mut self, until: Instant) -> Result<Next> {
    let received = match tokio::time::timeout_at(until, self.socket.next()).await {
      Err(_) => return Ok(Next::Timeout),
      Ok(received) => received?,
    };
    let text = match received {
      None => return Ok(Next::Closed),
      Some(Frame::Text(text)) => text,
      Some(Frame::Binary(data)) => String::from_utf8_lossy(&data).into_owned(),
    };
    let Ok(frame) = serde_json::from_str::<Value>(&text) else {
      return Ok(Next::Frame(Value::Null));
    };
    if frame.at("headers").is_object() {
      self.socket.send_json(&ack(&frame)).await?;
    }
    self.pushes.extend(pushed(&frame));
    Ok(Next::Frame(frame))
  }
}

fn closed() -> Error {
  Error::network("the chat server closed the connection")
}

/// What every frame with headers is answered with.
fn ack(frame: &Value) -> Value {
  let headers = frame.at("headers");
  let mut ack = json!({
    "mid": headers.get("mid").cloned().unwrap_or_else(|| Value::from(mid())),
    "sid": headers.get("sid").cloned().unwrap_or_else(|| Value::from("")),
  });
  for key in ["app-key", "ua", "dt"] {
    if let Some(value) = headers.get(key) {
      ack[key] = value.clone();
    }
  }
  json!({"code": 200, "headers": ack})
}

/// An answer's `code`: 200, or the error it stands for.
fn status(frame: &Value, lwp: &str) -> Result<()> {
  let code = frame.i64("code").unwrap_or(200);
  if code == 200 {
    return Ok(());
  }
  let reason = frame
    .first_str(&["body.reason", "body.message", "body.msg"])
    .unwrap_or_else(|| format!("code {code}"));
  if code == 401 {
    return Err(Error::auth(format!(
      "the chat server refused the session: {reason}"
    )));
  }
  Err(Error::upstream(format!(
    "chat request {lwp} failed: {reason}"
  )))
}

/// The messages a push frame carries, decoded. Entries that are JSON (or
/// base64 of JSON) report state changes and are skipped.
fn pushed(frame: &Value) -> Vec<Value> {
  frame
    .list("body.syncPushPackage.data")
    .iter()
    .filter_map(|entry| entry.get("data").and_then(Value::as_str))
    .filter(|data| serde_json::from_str::<Value>(data).is_err())
    .filter_map(|data| {
      let bytes = base64(data)?;
      if bytes.first() == Some(&b'{') {
        return None;
      }
      msgpack::decode(&bytes)
        .inspect_err(|e| tracing::debug!("undecodable push: {}", e.message))
        .ok()
    })
    .collect()
}

/// Standard or URL-safe base64, padded or not.
pub fn base64(data: &str) -> Option<Vec<u8>> {
  let data = data.trim();
  STANDARD.decode(data).ok().or_else(|| {
    let url_safe = data
      .trim_end_matches('=')
      .replace('+', "-")
      .replace('/', "_");
    URL_SAFE_NO_PAD.decode(url_safe).ok()
  })
}
