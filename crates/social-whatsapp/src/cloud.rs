//! The WhatsApp Business Platform Cloud API (`graph.facebook.com`), with a
//! system-user access token and a business phone number id.
//!
//! The Cloud API only sends: incoming messages reach a webhook, so it has no
//! chat list, no history and no `listen`. What it does have: sending text,
//! media and templates, reactions, read receipts, media download, the
//! business profile, phone numbers, templates and messaging analytics.

use std::collections::BTreeMap;
use std::path::Path;

use social_core::http::{Method, Part};
use social_core::{
  Action, Breakdown, Collection, Ctx, Error, ErrorCode, Insights, Media, MediaKind, Message, Page,
  Point, Result, Series, Share, User, Value, ValueExt, json,
};

/// Session extras of `login --token TOKEN:PHONE_NUMBER_ID[:WABA_ID]`.
pub const TOKEN: &str = "cloud_token";
pub const PHONE: &str = "cloud_phone";
pub const WABA: &str = "cloud_waba";
const GRAPH: &str = "https://graph.facebook.com";
const DEFAULT_VERSION: &str = "v23.0";

pub struct Cloud {
  token: String,
  phone: String,
  waba: Option<String>,
  version: String,
}

fn env(name: &str) -> Option<String> {
  std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

impl Cloud {
  /// Saved credentials, else `WHATSAPP_TOKEN` + `WHATSAPP_PHONE_NUMBER_ID`
  /// (+ `WHATSAPP_BUSINESS_ACCOUNT_ID`).
  pub fn from_ctx(ctx: &Ctx) -> Option<Self> {
    let saved = |k: &str| ctx.extra(k).filter(|v| !v.is_empty());
    let version = env("WHATSAPP_GRAPH_VERSION").unwrap_or_else(|| DEFAULT_VERSION.into());
    if let (Some(token), Some(phone)) = (saved(TOKEN), saved(PHONE)) {
      return Some(Self {
        token,
        phone,
        waba: saved(WABA),
        version,
      });
    }
    Some(Self {
      token: env("WHATSAPP_TOKEN")?,
      phone: env("WHATSAPP_PHONE_NUMBER_ID")?,
      waba: env("WHATSAPP_BUSINESS_ACCOUNT_ID"),
      version,
    })
  }

  /// `TOKEN:PHONE_NUMBER_ID[:WABA_ID]`.
  pub fn parse_token(s: &str) -> Result<(String, String, Option<String>)> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    let numeric = |p: &&str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    match parts.as_slice() {
      [token, phone] if numeric(phone) => Ok(((*token).into(), (*phone).into(), None)),
      [token, phone, waba] if numeric(phone) && numeric(waba) => {
        Ok(((*token).into(), (*phone).into(), Some((*waba).into())))
      }
      _ => Err(Error::input(
        "expected `ACCESS_TOKEN:PHONE_NUMBER_ID` or `ACCESS_TOKEN:PHONE_NUMBER_ID:WABA_ID` (from Meta's WhatsApp Manager / App Dashboard); personal accounts log in with `social whatsapp login` (QR code)",
      )),
    }
  }

  fn url(&self, path: &str) -> String {
    format!("{GRAPH}/{}/{}", self.version, path.trim_start_matches('/'))
  }

  async fn call(
    &self,
    ctx: &Ctx,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    body: Option<&Value>,
  ) -> Result<Value> {
    let mut req = ctx
      .http
      .request(method, self.url(path))
      .no_cookies()
      .header("authorization", format!("Bearer {}", self.token))
      .queries(query.iter().map(|(k, v)| (*k, v)));
    if let Some(b) = body {
      req = req.json(b);
    }
    check(req.send().await?.value()?)
  }

  async fn get(&self, ctx: &Ctx, path: &str, query: &[(&str, String)]) -> Result<Value> {
    self.call(ctx, Method::GET, path, query, None).await
  }

  async fn post(&self, ctx: &Ctx, path: &str, body: &Value) -> Result<Value> {
    self.call(ctx, Method::POST, path, &[], Some(body)).await
  }

  fn waba(&self) -> Result<&str> {
    self.waba.as_deref().ok_or_else(|| {
      Error::input("this needs the WhatsApp Business Account id").with_hint(
        "log in with `--token TOKEN:PHONE_NUMBER_ID:WABA_ID` or set WHATSAPP_BUSINESS_ACCOUNT_ID",
      )
    })
  }

  pub async fn whoami(&self, ctx: &Ctx) -> Result<User> {
    let v = self
      .get(
        ctx,
        &self.phone,
        &[(
          "fields",
          "display_phone_number,verified_name,quality_rating,code_verification_status,name_status,platform_type,throughput,messaging_limit_tier".into(),
        )],
      )
      .await?;
    let mut u = User {
      id: self.phone.clone(),
      name: v.str("verified_name").unwrap_or_default(),
      handle: v.str("display_phone_number"),
      ..User::default()
    };
    for (k, path) in [
      ("quality", "quality_rating"),
      ("verification", "code_verification_status"),
      ("name_status", "name_status"),
      ("platform", "platform_type"),
      ("throughput", "throughput.level"),
      ("messaging_limit", "messaging_limit_tier"),
    ] {
      if let Some(s) = v.str(path) {
        u.extra.insert(k.into(), s.into());
      }
    }
    u.extra.insert("mode".into(), "cloud-api".into());
    Ok(u)
  }

  /// Send text and files; the text becomes the first file's caption.
  pub async fn send(&self, ctx: &Ctx, to: &str, m: &Message) -> Result<Action> {
    let to = recipient(to)?;
    let context = m
      .reply_to
      .as_ref()
      .map(|r| json!({ "message_id": message_id(r) }));
    let mut caption = Some(m.text.clone()).filter(|t| !t.is_empty());
    let mut last = None;
    for path in &m.files {
      let (id, kind, name) = self.upload(ctx, path).await?;
      let mut media = json!({ "id": id });
      if kind != "audio"
        && kind != "sticker"
        && let Some(c) = caption.take()
      {
        media["caption"] = c.into();
      }
      if kind == "document" {
        media["filename"] = name.into();
      }
      let mut body = json!({
        "messaging_product": "whatsapp",
        "recipient_type": "individual",
        "to": to,
        "type": kind,
      });
      body[kind] = media;
      if let Some(c) = &context {
        body["context"] = c.clone();
      }
      last = self
        .post(ctx, &format!("{}/messages", self.phone), &body)
        .await?
        .str("messages.0.id");
    }
    if let Some(text) = caption {
      let mut body = json!({
        "messaging_product": "whatsapp",
        "recipient_type": "individual",
        "to": to,
        "type": "text",
        "text": { "body": text, "preview_url": true },
      });
      if let Some(c) = &context {
        body["context"] = c.clone();
      }
      last = self
        .post(ctx, &format!("{}/messages", self.phone), &body)
        .await?
        .str("messages.0.id");
    }
    let mut action = Action::done("send", format!("+{to}"));
    if let Some(id) = last {
      action = action.with_id(format!("{to}/{id}"));
    }
    Ok(action)
  }

  /// Upload a file for sending: `(media id, message type, file name)`.
  async fn upload(&self, ctx: &Ctx, path: &Path) -> Result<(String, &'static str, String)> {
    let data = std::fs::read(path)
      .map_err(|e| Error::input(format!("cannot read {}: {e}", path.display())))?;
    let name = path
      .file_name()
      .and_then(|n| n.to_str())
      .unwrap_or("file")
      .to_owned();
    let mime = crate::personal::mime_of(path, &data);
    let kind = match mime.split('/').next() {
      _ if mime == "image/webp" => "sticker",
      Some("image") => "image",
      Some("video") => "video",
      Some("audio") => "audio",
      _ => "document",
    };
    let resp = ctx
      .http
      .post(self.url(&format!("{}/media", self.phone)))
      .no_cookies()
      .header("authorization", format!("Bearer {}", self.token))
      .multipart(vec![
        Part::text("messaging_product", "whatsapp"),
        Part::text("type", mime.clone()),
        Part::file("file", data, &name, &mime),
      ])
      .send()
      .await?;
    let v = check(resp.value()?)?;
    let id = v
      .str("id")
      .ok_or_else(|| Error::upstream("the media upload returned no id"))?;
    Ok((id, kind, name))
  }

  /// `TO/WAMID` (as `send` prints it) or a bare `wamid.…` with `--to`.
  pub async fn react(&self, ctx: &Ctx, post: &str, emoji: &str, undo: bool) -> Result<Action> {
    let Some((to, id)) = post.rsplit_once('/') else {
      return Err(Error::input(
        "give the message as TO/MESSAGE_ID (as `send` prints it)",
      ));
    };
    let body = json!({
      "messaging_product": "whatsapp",
      "recipient_type": "individual",
      "to": recipient(to)?,
      "type": "reaction",
      "reaction": { "message_id": id, "emoji": if undo { "" } else { emoji } },
    });
    self
      .post(ctx, &format!("{}/messages", self.phone), &body)
      .await?;
    Ok(Action::done(if undo { "unreact" } else { "react" }, post))
  }

  /// Mark a received message (and those before it) as read.
  pub async fn mark_read(&self, ctx: &Ctx, message: &str) -> Result<Action> {
    let body = json!({
      "messaging_product": "whatsapp",
      "status": "read",
      "message_id": message_id(message),
    });
    self
      .post(ctx, &format!("{}/messages", self.phone), &body)
      .await?;
    Ok(Action::done("mark-read", message))
  }

  /// A media id (from a webhook payload) as a downloadable media.
  pub async fn media(&self, ctx: &Ctx, media_id: &str) -> Result<Media> {
    let v = self.get(ctx, media_id, &[]).await?;
    let url = v
      .str("url")
      .ok_or_else(|| Error::not_found("no such media (Cloud API media live 30 days)"))?;
    let mime = v.str("mime_type").unwrap_or_default();
    let kind = match mime.split('/').next() {
      Some("image") => MediaKind::Image,
      Some("video") => MediaKind::Video,
      Some("audio") => MediaKind::Audio,
      _ => MediaKind::File,
    };
    let mut m = Media::new(kind, url);
    m.size = v.u64("file_size");
    m.headers = vec![("authorization".into(), format!("Bearer {}", self.token))];
    Ok(m)
  }

  pub async fn profile(&self, ctx: &Ctx) -> Result<Value> {
    let v = self
      .get(
        ctx,
        &format!("{}/whatsapp_business_profile", self.phone),
        &[(
          "fields",
          "about,address,description,email,profile_picture_url,websites,vertical".into(),
        )],
      )
      .await?;
    Ok(v.at("data.0").clone())
  }

  pub async fn numbers(&self, ctx: &Ctx) -> Result<Page<User>> {
    let v = self
      .get(
        ctx,
        &format!("{}/phone_numbers", self.waba()?),
        &[(
          "fields",
          "id,display_phone_number,verified_name,quality_rating,code_verification_status,name_status,messaging_limit_tier".into(),
        )],
      )
      .await?;
    let users = v
      .list("data")
      .iter()
      .map(|n| {
        let mut u = User {
          id: n.str("id").unwrap_or_default(),
          name: n.str("verified_name").unwrap_or_default(),
          handle: n.str("display_phone_number"),
          ..User::default()
        };
        for (k, p) in [
          ("quality", "quality_rating"),
          ("verification", "code_verification_status"),
          ("name_status", "name_status"),
          ("messaging_limit", "messaging_limit_tier"),
        ] {
          if let Some(s) = n.str(p) {
            u.extra.insert(k.into(), s.into());
          }
        }
        u
      })
      .collect();
    Ok(Page::new(users, None))
  }

  pub async fn templates(&self, ctx: &Ctx) -> Result<Page<Collection>> {
    let v = self
      .get(
        ctx,
        &format!("{}/message_templates", self.waba()?),
        &[
          (
            "fields",
            "id,name,status,category,language,components,quality_score".into(),
          ),
          ("limit", "200".into()),
        ],
      )
      .await?;
    let items = v
      .list("data")
      .iter()
      .map(|t| {
        let body = t
          .list("components")
          .iter()
          .find(|c| c.str("type").as_deref() == Some("BODY"))
          .and_then(|c| c.str("text"));
        let mut c = Collection {
          id: t.str("id").unwrap_or_default(),
          kind: "template".into(),
          name: t.str("name").unwrap_or_default(),
          description: body,
          raw: Some(t.clone()),
          ..Collection::default()
        };
        for (k, p) in [
          ("status", "status"),
          ("category", "category"),
          ("language", "language"),
          ("quality", "quality_score.score"),
        ] {
          if let Some(s) = t.str(p) {
            c.extra.insert(k.into(), s.into());
          }
        }
        c
      })
      .collect();
    Ok(Page::new(items, None))
  }

  /// Send an approved template (the only way to open a conversation).
  pub async fn send_template(
    &self,
    ctx: &Ctx,
    to: &str,
    name: &str,
    language: &str,
    params: &[String],
  ) -> Result<Action> {
    let to = recipient(to)?;
    let mut template = json!({ "name": name, "language": { "code": language } });
    if !params.is_empty() {
      let parameters: Vec<Value> = params
        .iter()
        .map(|p| json!({ "type": "text", "text": p }))
        .collect();
      template["components"] = json!([{ "type": "body", "parameters": parameters }]);
    }
    let body = json!({
      "messaging_product": "whatsapp",
      "recipient_type": "individual",
      "to": to,
      "type": "template",
      "template": template,
    });
    let v = self
      .post(ctx, &format!("{}/messages", self.phone), &body)
      .await?;
    let mut action = Action::done("send-template", format!("+{to}"));
    if let Some(id) = v.str("messages.0.id") {
      action = action.with_id(format!("{to}/{id}"));
    }
    Ok(action)
  }

  /// Messages sent / delivered per day, and pricing analytics per category.
  pub async fn insights(&self, ctx: &Ctx, days: u32) -> Result<Insights> {
    let waba = self.waba()?.to_owned();
    let end = jiff::Timestamp::now().as_second();
    let start = end - i64::from(days.clamp(1, 90)) * 86_400;
    let mut ins = Insights {
      kind: "account".into(),
      subject: self.phone.clone(),
      title: Some("WhatsApp Business messaging".into()),
      from: social_core::text::from_secs(start).map(|t| day(&t)),
      to: social_core::text::from_secs(end).map(|t| day(&t)),
      totals_period: Some(format!("{days}d")),
      ..Insights::default()
    };
    let analytics = self
      .get(
        ctx,
        &waba,
        &[(
          "fields",
          format!("analytics.start({start}).end({end}).granularity(DAY)"),
        )],
      )
      .await;
    match analytics {
      Ok(v) => {
        let points = v.list("analytics.data_points");
        for metric in ["sent", "delivered"] {
          let series: Vec<Point> = points
            .iter()
            .filter_map(|p| {
              let date = social_core::text::from_secs(p.i64("start")?).map(|t| day(&t))?;
              Some(Point {
                date,
                value: p.u64(metric).unwrap_or(0).into(),
              })
            })
            .collect();
          let total: u64 = points.iter().filter_map(|p| p.u64(metric)).sum();
          ins.total(&format!("messages_{metric}"), Some(Value::from(total)));
          ins.series.push(Series {
            metric: format!("messages_{metric}"),
            points: series,
          });
        }
      }
      Err(e) => ins
        .warnings
        .push(format!("message analytics: {}", e.message)),
    }
    let pricing = self
      .get(
        ctx,
        &waba,
        &[(
          "fields",
          format!(
            "pricing_analytics.start({start}).end({end}).granularity(DAILY).dimensions([\"PRICING_CATEGORY\"])"
          ),
        )],
      )
      .await;
    match pricing {
      Ok(v) => {
        let mut volume: BTreeMap<String, u64> = BTreeMap::new();
        let mut cost = 0.0;
        for d in v.list("pricing_analytics.data") {
          for p in d.list("data_points") {
            let category = p.str("pricing_category").unwrap_or_else(|| "other".into());
            *volume.entry(category.to_lowercase()).or_default() += p.u64("volume").unwrap_or(0);
            cost += p.f64("cost").unwrap_or(0.0);
          }
        }
        ins.total("cost", Some(json!(cost)));
        ins.breakdowns.push(Breakdown {
          dimension: "pricing_category".into(),
          items: volume
            .into_iter()
            .map(|(label, n)| Share {
              label,
              value: n.into(),
              ..Share::default()
            })
            .collect(),
          ..Breakdown::default()
        });
      }
      Err(e) => ins
        .warnings
        .push(format!("pricing analytics: {}", e.message)),
    }
    Ok(ins)
  }
}

fn day(t: &jiff::Timestamp) -> String {
  t.strftime("%Y-%m-%d").to_string()
}

/// A recipient's number in Cloud API form (digits with country code).
fn recipient(to: &str) -> Result<String> {
  let to = to.split('@').next().unwrap_or(to);
  crate::refs::phone(to).ok_or_else(|| {
    Error::input(format!(
      "not a phone number: {to} (give it with its country code, e.g. +44 7700 900123)"
    ))
  })
}

/// The `wamid.…` part of `TO/WAMID`.
fn message_id(r: &str) -> &str {
  r.rsplit_once('/').map_or(r, |(_, id)| id)
}

/// Graph API errors into the shared error codes.
fn check(v: Value) -> Result<Value> {
  let Some(e) = v.get("error").filter(|e| e.is_object()) else {
    return Ok(v);
  };
  let code = e.i64("code").unwrap_or_default();
  let detail = e
    .str("error_data.details")
    .or_else(|| e.str("message"))
    .unwrap_or_else(|| "unknown error".into());
  let message = format!("Cloud API error {code}: {detail}");
  Err(match code {
    190 => Error::auth(message).with_hint("the access token expired or is invalid; create a system-user token in Meta Business Manager"),
    10 | 200..=299 => Error::new(ErrorCode::PermissionDenied, message)
      .with_hint("the token lacks whatsapp_business_messaging / whatsapp_business_management"),
    4 | 80007 | 130429 | 131056 => Error::new(ErrorCode::RateLimited, message),
    131047 => Error::new(ErrorCode::PermissionDenied, message).with_hint(
      "outside the 24-hour customer service window only templates can be sent: `social whatsapp template TO NAME`",
    ),
    100 | 131008 | 131009 => Error::input(message),
    _ => Error::upstream(message),
  })
}
