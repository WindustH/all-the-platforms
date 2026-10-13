//! The official TikTok Marketing (Business) API, for ad accounts:
//! `https://business-api.tiktok.com/open_api/v1.3`, authorized by an access
//! token in the `Access-Token` header.
//!
//! The token comes from `TIKTOK_ADS_ACCESS_TOKEN` or `login --token` (kept
//! with the session). Listing the advertisers a token may manage also needs
//! the app's `TIKTOK_ADS_APP_ID` and `TIKTOK_ADS_SECRET`. The advertiser to
//! work on: `-a`, `TIKTOK_ADS_ADVERTISER_ID`, or the one saved by `ads use`.

mod commands;

pub use commands::{Command, run};

use social_core::{Error, ErrorCode, Result, User, Value, ValueExt, json};

use crate::api::Api;

const BASE: &str = "https://business-api.tiktok.com/open_api/v1.3";
pub const TOKEN_KEY: &str = "ads_token";
const ADVERTISER_KEY: &str = "ads_advertiser";
const TOKEN_HINT: &str = "set TIKTOK_ADS_ACCESS_TOKEN or run `social tiktok login --token <access token>` \
   (a long-term token of a Marketing API app from https://business-api.tiktok.com)";

fn env(name: &str) -> Option<String> {
  std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// Whether an access token is configured.
pub fn configured(api: &Api) -> bool {
  token(api).is_some()
}

fn token(api: &Api) -> Option<String> {
  env("TIKTOK_ADS_ACCESS_TOKEN").or_else(|| api.ctx.extra(TOKEN_KEY))
}

pub struct Ads<'a> {
  api: &'a Api,
  token: String,
}

impl<'a> Ads<'a> {
  pub fn new(api: &'a Api) -> Result<Self> {
    let token = token(api)
      .ok_or_else(|| Error::auth("no TikTok Marketing API access token").with_hint(TOKEN_HINT))?;
    Ok(Self { api, token })
  }

  /// The advertiser to use: given, from the environment, or saved by `ads use`.
  pub fn advertiser(&self, given: Option<&str>) -> Result<String> {
    given
      .map(str::to_owned)
      .or_else(|| env("TIKTOK_ADS_ADVERTISER_ID"))
      .or_else(|| self.api.ctx.extra(ADVERTISER_KEY))
      .ok_or_else(|| {
        Error::input("which advertiser? pass -a ID, set TIKTOK_ADS_ADVERTISER_ID or run `social tiktok ads use ID`")
          .with_hint("`social tiktok ads advertisers` lists the ones your token manages")
      })
  }

  pub fn remember_advertiser(&self, id: &str) {
    self.api.ctx.set_extra(ADVERTISER_KEY, id);
  }

  /// GET an endpoint; non-string values are sent JSON-encoded, as the API expects.
  pub async fn get(&self, path: &str, params: &[(&str, Value)]) -> Result<Value> {
    let mut req = self
      .api
      .ctx
      .http
      .get(format!("{BASE}{path}"))
      .no_cookies()
      .header("access-token", &self.token);
    for (k, v) in params {
      if v.is_null() {
        continue;
      }
      let text = match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
      };
      req = req.query(k, text);
    }
    check(req.send().await?.value()?)
  }

  pub async fn post(&self, path: &str, body: &Value) -> Result<Value> {
    let req = self
      .api
      .ctx
      .http
      .post(format!("{BASE}{path}"))
      .no_cookies()
      .header("access-token", &self.token)
      .json(body);
    check(req.send().await?.value()?)
  }

  /// Every `data.list` entry of a paged listing, up to `limit` (0 = all).
  pub async fn list(
    &self,
    path: &str,
    params: &[(&str, Value)],
    limit: usize,
  ) -> Result<Vec<Value>> {
    let size = if limit == 0 {
      1000
    } else {
      limit.clamp(1, 1000)
    };
    let mut out = Vec::new();
    let mut page = 1u64;
    loop {
      let mut p = params.to_vec();
      p.push(("page", json!(page)));
      p.push(("page_size", json!(size)));
      let data = self.get(path, &p).await?;
      let rows = data.list("list");
      out.extend(rows.iter().cloned());
      let pages = data.u64("page_info.total_page").unwrap_or(1);
      if rows.is_empty() || page >= pages || (limit > 0 && out.len() >= limit) {
        break;
      }
      page += 1;
    }
    if limit > 0 {
      out.truncate(limit);
    }
    Ok(out)
  }

  /// The person the token belongs to.
  pub async fn me(&self) -> Result<User> {
    let v = self.get("/user/info/", &[]).await?;
    Ok(User {
      id: v
        .str("core_user_id")
        .or_else(|| v.str("id"))
        .unwrap_or_default(),
      name: v.str("display_name").unwrap_or_default(),
      handle: v.str("email"),
      avatar: v.str("avatar_url"),
      extra: [("source".to_owned(), Value::from("marketing-api"))].into(),
      raw: Some(v),
      ..User::default()
    })
  }
}

/// `{code, message, data}`: `data` when `code` is 0.
fn check(v: Value) -> Result<Value> {
  let code = v.i64("code").unwrap_or(-1);
  if code == 0 {
    return Ok(v.at("data").clone());
  }
  let msg = v.str("message").unwrap_or_else(|| "unknown error".into());
  let text = format!("TikTok Marketing API error {code}: {msg}");
  Err(match code {
    40100 | 51021 => Error::new(ErrorCode::RateLimited, text),
    40102 | 40104 | 40105 => Error::auth(text).with_hint(TOKEN_HINT),
    40001 | 40002 | 40006 => Error::new(ErrorCode::PermissionDenied, text),
    40000 | 40003 | 40004 | 40005 | 40007 | 40008 => Error::input(text),
    _ => Error::upstream(text),
  })
}
