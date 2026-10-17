//! Publishing: image upload (register → OSS → poll), pins (想法), questions
//! and articles; changes to answers and pins in [`edit`].

mod edit;

use std::path::PathBuf;

use atp_core::http::Method;
use atp_core::{Action, Error, Result, Value, ValueExt, json};

use crate::api::{Api, V4, WWW, ZHUANLAN};
use crate::markup;
use crate::refs::{answer_url, article_url, question_url};
use crate::uploads::{Image, upload_all};

pub use self::edit::edit;

/// Plain text as HTML paragraphs, one per line.
fn paragraphs(text: &str) -> String {
  text
    .lines()
    .map(str::trim)
    .filter(|l| !l.is_empty())
    .map(|l| format!("<p>{l}</p>"))
    .collect()
}

fn images_html(images: &[Image]) -> String {
  images.iter().map(Image::html).collect()
}

/// A body as editor HTML with its length in characters: plain text as one
/// paragraph per line, or `markdown` with its images uploaded in place.
/// The flag says whether the HTML holds uploaded images.
async fn body_html(
  api: &Api,
  body: &str,
  markdown: bool,
  source: &str,
) -> Result<(String, usize, bool)> {
  if !markdown {
    return Ok((paragraphs(body), body.chars().count(), false));
  }
  let m = markup::render(body);
  if let Some(missing) = m.images.iter().find(|p| !p.is_file()) {
    return Err(Error::input(format!(
      "image not found: {}",
      missing.display()
    )));
  }
  let images = upload_all(api, &m.images, source).await?;
  let html: Vec<String> = images.iter().map(Image::html).collect();
  Ok((m.fill(&html), m.text_len, !images.is_empty()))
}

async fn content_draft(api: &Api, action: &str) -> Result<String> {
  let v = api
    .call(
      api
        .post(&format!("{V4}/content/drafts"))
        .json(&json!({ "action": action })),
    )
    .await?;
  v.str("data.content_id")
    .ok_or_else(|| Error::upstream("draft created but no content_id returned"))
}

/// The unified editor endpoint; returns the id of the published content.
async fn content_publish(api: &Api, payload: &Value) -> Result<String> {
  let v = api
    .call(api.post(&format!("{V4}/content/publish")).json(payload))
    .await?;
  if let Some(code) = v.i64("code").filter(|c| *c != 0) {
    let message = v
      .first_str(&["message", "toast_message"])
      .unwrap_or_else(|| format!("code {code}"));
    return Err(Error::upstream(format!("publish failed: {message}")));
  }
  let result: Value = v
    .str("data.result")
    .and_then(|r| serde_json::from_str(&r).ok())
    .unwrap_or(Value::Null);
  result
    .first_str(&["id", "data.id"])
    .or_else(|| v.first_str(&["data.id", "id"]))
    .ok_or_else(|| Error::upstream("published, but Zhihu returned no id"))
}

fn trace_id() -> String {
  let ms = jiff::Timestamp::now().as_millisecond();
  let b: [u8; 16] = rand::random();
  let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
  // A version-4 UUID shape, like the web editor sends.
  format!(
    "{ms},{}-{}-4{}-a{}-{}",
    &hex[..8],
    &hex[8..12],
    &hex[13..16],
    &hex[17..20],
    &hex[20..32]
  )
}

/// Publish a pin (想法) with an optional title and images.
pub async fn pin(api: &Api, title: &str, text: &str, images: &[PathBuf]) -> Result<Action> {
  api.prepare_write().await?;
  let images = upload_all(api, images, "pin").await?;
  let draft = content_draft(api, "pin").await?;
  let chars = text.chars().count();
  let mut data = json!({
    "publish": { "traceId": trace_id() },
    "commentsPermission": { "comment_permission": "all" },
    "extra_info": { "view_permission": "all", "publisher": "pc" },
    "draft": { "disabled": 1, "id": draft },
    "title": { "title": title },
    "hybrid": { "html": paragraphs(text) + &images_html(&images), "textLength": chars },
  });
  if !images.is_empty() {
    data["hybrid"]["textLength"] = (title.chars().count() + chars).into();
    let medias: Vec<Value> = images
      .iter()
      .map(|i| {
        json!({ "image": {
          "width": i.width, "height": i.height, "url": i.src, "originalUrl": i.original,
          "watermark": i.watermark, "watermarkUrl": i.watermark_src,
        }})
      })
      .collect();
    data["media"] = json!({ "medias": medias });
  }
  let id = content_publish(api, &json!({ "action": "pin", "data": data })).await?;
  Ok(
    Action::done("publish", "pin")
      .with_id(&id)
      .with_url(format!("{WWW}/pin/{id}")),
  )
}

/// Ask a question; `topics` are topic ids.
pub async fn question(
  api: &Api,
  title: &str,
  detail: &str,
  topics: &[String],
  images: &[PathBuf],
) -> Result<Action> {
  api.prepare_write().await?;
  let detail_html = paragraphs(detail);
  let id = if images.is_empty() {
    let v = api
      .call(api.post(&format!("{V4}/questions")).json(&json!({
        "title": title, "detail": detail_html, "topic_url_tokens": topics,
      })))
      .await?;
    v.str("id")
      .ok_or_else(|| Error::upstream("question created, but Zhihu returned no id"))?
  } else {
    let images = upload_all(api, images, "question").await?;
    let data = json!({
      "title": { "title": title },
      "topic": { "topics": topics },
      "hybrid": { "html": detail_html + &images_html(&images), "textLength": detail.chars().count() },
      "extra_info": { "publisher": "pc" },
      "questionConfig": { "type": "0" },
      "draft": { "disabled": 1 },
    });
    content_publish(api, &json!({ "action": "question", "data": data })).await?
  };
  Ok(
    Action::done("ask", "question")
      .with_url(question_url(&id))
      .with_id(id),
  )
}

/// Publish a column article; `topics` are topic ids.
pub async fn article(
  api: &Api,
  title: &str,
  body: &str,
  markdown: bool,
  topics: &[String],
  images: &[PathBuf],
) -> Result<Action> {
  api.prepare_write().await?;
  let (html, text_len, inline_images) = body_html(api, body, markdown, "article").await?;
  let id = if images.is_empty() && !inline_images {
    let draft = api
      .call(
        api
          .post(&format!("{ZHUANLAN}/articles/drafts"))
          .json(&json!({})),
      )
      .await?;
    let id = draft
      .str("id")
      .ok_or_else(|| Error::upstream("article draft created, but Zhihu returned no id"))?;
    let mut patch = json!({ "title": title, "content": html });
    if !topics.is_empty() {
      patch["topics"] = json!(topics);
    }
    let url = format!("{ZHUANLAN}/articles/{id}/draft");
    api
      .call(api.request(Method::PATCH, &url).json(&patch))
      .await?;
    let url = format!("{ZHUANLAN}/articles/{id}/publish");
    let body = json!({ "column": null, "commentPermission": "anyone" });
    let v = api.call(api.request(Method::PUT, &url).json(&body)).await?;
    v.str("id").unwrap_or(id)
  } else {
    let images = upload_all(api, images, "article").await?;
    let draft = content_draft(api, "article").await?;
    let data = json!({
      "title": { "title": title },
      "hybrid": { "html": html + &images_html(&images), "textLength": text_len },
      "extra_info": { "publisher": "pc" },
      "draft": { "disabled": 1, "id": draft },
      "commentsPermission": { "comment_permission": "anyone" },
    });
    content_publish(api, &json!({ "action": "article", "data": data })).await?
  };
  Ok(
    Action::done("publish", "article")
      .with_url(article_url(&id))
      .with_id(id),
  )
}

/// The answer editor's settings (`getSubmitData`). A new answer sets them
/// all; a changed one leaves reposting and comments out, so they stay.
struct AnswerForm {
  reshipment: Option<String>,
  comments: Option<String>,
  can_reward: bool,
  /// The reward request (赞赏语), when rewards are on.
  tagline: String,
  disclaimer_status: String,
  disclaimer_type: String,
  /// The id of the inviter thanked under the answer; empty for none.
  thank_inviter: String,
  /// Declared a commercial report (商业报备).
  is_report: bool,
  /// `content_source` of the answer (`hybridInfo.contentSource`).
  content_source: Value,
}

impl Default for AnswerForm {
  fn default() -> Self {
    Self {
      reshipment: Some("allowed".into()),
      comments: Some("all".into()),
      can_reward: false,
      tagline: String::new(),
      disclaimer_status: "close".into(),
      disclaimer_type: "none".into(),
      thank_inviter: String::new(),
      is_report: false,
      content_source: Value::Null,
    }
  }
}

impl AnswerForm {
  fn reward(&self) -> Value {
    let mut reward = json!({ "can_reward": self.can_reward });
    if !self.tagline.is_empty() {
      reward["tagline"] = self.tagline.clone().into();
    }
    reward
  }

  fn thanks(&self) -> Value {
    let status = if self.thank_inviter.is_empty() {
      "close"
    } else {
      "open"
    };
    json!({ "thank_inviter_status": status, "thank_inviter": self.thank_inviter })
  }

  /// `pc_business_params`: the settings in snake case, as one JSON text.
  fn business(&self) -> String {
    let mut business = serde_json::Map::new();
    if let Some(setting) = &self.reshipment {
      business.insert("reshipment_settings".into(), setting.clone().into());
    }
    if let Some(setting) = &self.comments {
      business.insert("comment_permission".into(), setting.clone().into());
    }
    let rest = json!({
      "reward_setting": self.reward(), "disclaimer_status": self.disclaimer_status,
      "disclaimer_type": self.disclaimer_type,
      "commercial_report_info": { "is_report": self.is_report },
      "commercial_zhitask_bind_info": null, "is_report": self.is_report,
      "table_of_contents_enabled": false,
    });
    for part in [rest, self.thanks()] {
      business.extend(
        part
          .as_object()
          .into_iter()
          .flatten()
          .map(|(k, v)| (k.clone(), v.clone())),
      );
    }
    if !self.content_source.is_null() {
      business.insert("content_source".into(), self.content_source.clone());
    }
    Value::Object(business).to_string()
  }
}

/// The `data` of `content/publish` for an answer (the editor's `w`):
/// `draft` says whether it is new or which answer it changes.
fn answer_data(
  question: &str,
  html: String,
  text_len: usize,
  form: &AnswerForm,
  draft: Value,
) -> Value {
  let setting = |key: &str, value: &Option<String>| match value {
    Some(value) => json!({ key: value }),
    None => json!({}),
  };
  let mut hybrid_info = json!({});
  if !form.content_source.is_null() {
    hybrid_info["contentSource"] = form.content_source.clone();
  }
  json!({
    "publish": { "traceId": trace_id() },
    "hybridInfo": hybrid_info,
    "draft": draft,
    "extra_info": {
      "question_id": question, "publisher": "pc", "pc_business_params": form.business(),
    },
    "hybrid": { "html": html, "textLength": text_len },
    "reprint": setting("reshipment_settings", &form.reshipment),
    "commentsPermission": setting("comment_permission", &form.comments),
    "appreciate": form.reward(),
    "publishSwitch": { "draft_type": "normal" },
    "creationStatement": {
      "disclaimer_status": form.disclaimer_status, "disclaimer_type": form.disclaimer_type,
    },
    "commercialReportInfo": { "isReport": i32::from(form.is_report) },
    "toFollower": {},
    "contentsTables": { "table_of_contents_enabled": false },
    "thanksInvitation": form.thanks(),
  })
}

/// Answer a question through the editor's publish endpoint.
pub async fn answer(
  api: &Api,
  question: &str,
  body: &str,
  markdown: bool,
  images: &[PathBuf],
) -> Result<Action> {
  api.prepare_write().await?;
  let (html, text_len, _) = body_html(api, body, markdown, "answer").await?;
  let images = upload_all(api, images, "answer").await?;
  let html = html + &images_html(&images);
  let draft = json!({ "isPublished": false, "disabled": 1 });
  let data = answer_data(question, html, text_len, &AnswerForm::default(), draft);
  let id = content_publish(api, &json!({ "action": "answer", "data": data })).await?;
  Ok(
    Action::done("publish", "answer")
      .with_url(answer_url(&id, Some(question)))
      .with_id(id),
  )
}
