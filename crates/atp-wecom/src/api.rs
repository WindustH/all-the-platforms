//! WeCom (企业微信) transport: `qyapi.weixin.qq.com/cgi-bin` with a corp
//! app's access token (`corpid` + the app's secret), and the AI bot's
//! credentials for the long connection.
//!
//! Credentials: `login --token CORPID:SECRET[:AGENTID]` for an app (the
//! agent id is needed to send), `login --token bot:BOTID:SECRET` for an AI
//! bot (智能机器人), or WECOM_CORP_ID / WECOM_SECRET / WECOM_AGENT_ID and
//! WECOM_BOT_ID / WECOM_BOT_SECRET, which win over the saved ones. Both
//! kinds can be kept at once.

use std::time::Duration;

use atp_core::credentials;
use atp_core::http::{Part, Req};
use atp_core::token::AccessToken;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};
use md5::{Digest, Md5};

pub const API: &str = "https://qyapi.weixin.qq.com/cgi-bin";
const CORP: &str = "corp_id";
const SECRET: &str = "secret";
const AGENT: &str = "agent_id";
const BOT: &str = "bot_id";
const BOT_SECRET: &str = "bot_secret";
/// Codes that mean the access token is missing, invalid or expired.
const STALE_TOKEN: [i64; 4] = [40001, 40014, 42001, 41001];
pub const LOGIN_HINT: &str = "run `atp wecom login --token CORPID:SECRET:AGENTID` (an app) or \
   `atp wecom login --token bot:BOTID:SECRET` (an AI bot), or set WECOM_CORP_ID / WECOM_SECRET / \
   WECOM_AGENT_ID; robot webhooks need no login";

#[derive(Debug, Clone)]
pub struct App {
  pub corp_id: String,
  pub secret: String,
  pub agent_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Bot {
  pub bot_id: String,
  pub secret: String,
}

/// What `login --token` was given.
pub enum Login {
  App(App),
  Bot(Bot),
}

impl Login {
  pub fn parse(raw: &str) -> Result<Self> {
    let fields = credentials::fields(raw);
    let fields: Vec<&str> = fields.iter().map(String::as_str).collect();
    match fields.as_slice() {
      ["bot", id, secret] if !id.is_empty() && !secret.is_empty() => Ok(Login::Bot(Bot {
        bot_id: (*id).to_owned(),
        secret: (*secret).to_owned(),
      })),
      ["bot", ..] => Err(Error::input("expected bot:BOTID:SECRET").with_hint(LOGIN_HINT)),
      [corp, secret, agent @ ..] if !corp.is_empty() && !secret.is_empty() && agent.len() <= 1 => {
        Ok(Login::App(App {
          corp_id: (*corp).to_owned(),
          secret: (*secret).to_owned(),
          agent_id: agent
            .first()
            .filter(|a| !a.is_empty())
            .map(|a| (*a).to_owned()),
        }))
      }
      _ => Err(Error::input("expected CORPID:SECRET[:AGENTID]").with_hint(LOGIN_HINT)),
    }
  }
}

pub enum Body {
  Get,
  Json(Value),
  Multipart(Vec<Part>),
}

pub struct Api {
  pub ctx: Ctx,
  env_app: Option<App>,
  env_bot: Option<Bot>,
  token: AccessToken,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let env_app = match (
      credentials::env("WECOM_CORP_ID"),
      credentials::env("WECOM_SECRET"),
    ) {
      (Some(corp_id), Some(secret)) => Some(App {
        corp_id,
        secret,
        agent_id: credentials::env("WECOM_AGENT_ID"),
      }),
      _ => None,
    };
    let env_bot = match (
      credentials::env("WECOM_BOT_ID"),
      credentials::env("WECOM_BOT_SECRET"),
    ) {
      (Some(bot_id), Some(secret)) => Some(Bot { bot_id, secret }),
      _ => None,
    };
    Self {
      ctx,
      env_app,
      env_bot,
      token: AccessToken::default(),
    }
  }

  pub fn app(&self) -> Option<App> {
    if let Some(app) = &self.env_app {
      return Some(app.clone());
    }
    let [corp_id, secret] = credentials::saved(&self.ctx, &[CORP, SECRET])?
      .try_into()
      .ok()?;
    Some(App {
      corp_id,
      secret,
      agent_id: self.ctx.extra(AGENT).filter(|v| !v.is_empty()),
    })
  }

  pub fn bot(&self) -> Option<Bot> {
    if let Some(bot) = &self.env_bot {
      return Some(bot.clone());
    }
    let [bot_id, secret] = credentials::saved(&self.ctx, &[BOT, BOT_SECRET])?
      .try_into()
      .ok()?;
    Some(Bot { bot_id, secret })
  }

  pub fn logged_in(&self) -> bool {
    self.app().is_some() || self.bot().is_some()
  }

  pub fn require_app(&self) -> Result<App> {
    self
      .app()
      .ok_or_else(|| Error::auth("no WeCom app credentials").with_hint(LOGIN_HINT))
  }

  pub fn require_agent(&self) -> Result<(App, String)> {
    let app = self.require_app()?;
    let agent = app.agent_id.clone().ok_or_else(|| {
      Error::input("sending as the app needs its agent id")
        .with_hint("`atp wecom login --token CORPID:SECRET:AGENTID`, or set WECOM_AGENT_ID")
    })?;
    Ok((app, agent))
  }

  pub fn save(&self, login: &Login) {
    match login {
      Login::App(app) => credentials::save(
        &self.ctx,
        &[
          (CORP, &app.corp_id),
          (SECRET, &app.secret),
          (AGENT, app.agent_id.as_deref().unwrap_or_default()),
        ],
      ),
      Login::Bot(bot) => {
        credentials::save(&self.ctx, &[(BOT, &bot.bot_id), (BOT_SECRET, &bot.secret)])
      }
    }
  }

  /// The app's access token: memory, cache, or `gettoken`.
  pub async fn token(&self) -> Result<String> {
    let app = self.require_app()?;
    self
      .token
      .get(&self.ctx, &token_key(&app), async || {
        self.fetch_token(&app).await
      })
      .await
  }

  async fn fetch_token(&self, app: &App) -> Result<(String, i64)> {
    let v: Value = self
      .ctx
      .http
      .get(format!("{API}/gettoken"))
      .no_cookies()
      .query("corpid", &app.corp_id)
      .query("corpsecret", &app.secret)
      .value()
      .await?;
    let v = check(v, "/gettoken")?;
    let token = v
      .str("access_token")
      .ok_or_else(|| Error::upstream("no access_token in the reply"))?;
    Ok((token, v.i64("expires_in").unwrap_or(7200)))
  }

  fn forget_token(&self) {
    if let Some(app) = self.app() {
      self.token.forget(&self.ctx, &token_key(&app));
    }
  }

  pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    self.call(path, query, Body::Get).await
  }

  pub async fn post(&self, path: &str, body: Value) -> Result<Value> {
    self.call(path, &[], Body::Json(body)).await
  }

  /// An app call with the access token, renewed once when WeCom refuses it.
  pub async fn call(&self, path: &str, query: &[(&str, String)], body: Body) -> Result<Value> {
    for attempt in 0..2 {
      let token = self.token().await?;
      let v: Value = self
        .request(&format!("{API}{path}"), &body)
        .query("access_token", &token)
        .queries(query.iter().map(|(k, v)| (*k, v)))
        .value()
        .await?;
      match v.i64("errcode") {
        Some(code) if STALE_TOKEN.contains(&code) && attempt == 0 => self.forget_token(),
        _ => return check(v, path),
      }
    }
    unreachable!("the second attempt always returns")
  }

  /// A group robot webhook call (`/webhook/send`, `/webhook/upload_media`):
  /// the robot's key stands in for a token.
  pub async fn webhook(
    &self,
    path: &str,
    key: &str,
    query: &[(&str, String)],
    body: Body,
  ) -> Result<Value> {
    let v: Value = self
      .request(&format!("{API}{path}"), &body)
      .query("key", key)
      .queries(query.iter().map(|(k, v)| (*k, v)))
      .value()
      .await?;
    check(v, path)
  }

  fn request(&self, url: &str, body: &Body) -> Req<'_> {
    let req = match body {
      Body::Get => self.ctx.http.get(url),
      Body::Json(v) => self.ctx.http.post(url).json(v),
      Body::Multipart(parts) => self
        .ctx
        .http
        .post(url)
        .multipart(parts.clone())
        .timeout(Duration::from_secs(300)),
    };
    req.no_cookies()
  }
}

/// Tokens are per app secret; the cache key names the corp and a hash.
fn token_key(app: &App) -> String {
  let hash = Md5::digest(app.secret.as_bytes());
  let short: String = hash.iter().take(4).map(|b| format!("{b:02x}")).collect();
  format!("token-{}-{short}", app.corp_id)
}

/// `errcode` 0 passes; the rest become errors saying what to do.
pub fn check(v: Value, path: &str) -> Result<Value> {
  let code = v.i64("errcode").unwrap_or(0);
  if code == 0 {
    return Ok(v);
  }
  let msg = v.str("errmsg").unwrap_or_default();
  let msg = msg
    .split(", hint:")
    .next()
    .unwrap_or(&msg)
    .trim()
    .to_owned();
  let detail = format!("{path}: {code} {msg}");
  Err(match code {
    40013 | 40001 | 40014 | 42001 | 41001 | 40091 | 40056 => {
      Error::auth(format!("WeCom refused the credentials ({detail})")).with_hint(LOGIN_HINT)
    }
    60020 => Error::new(
      ErrorCode::IpBlocked,
      format!("this IP is not trusted by the app ({detail})"),
    )
    .with_hint("add this machine's public IP to the app's 企业可信IP in the WeCom admin console"),
    48002 | 60011 | 301002 | 48001 | 60124 => Error::new(
      ErrorCode::PermissionDenied,
      format!("the app may not do this ({detail})"),
    )
    .with_hint("check the app's 可见范围 and API permissions in the WeCom admin console"),
    45009 | 45033 | 45047 => Error::new(ErrorCode::RateLimited, detail),
    93000 | 93004 => {
      Error::not_found(detail).with_hint("the robot webhook key is wrong or the robot was removed")
    }
    60111 | 60123 | 86003 | 95016 | 95018 => Error::not_found(detail),
    _ => Error::upstream(detail),
  })
}
