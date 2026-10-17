//! Changing an answer or a pin (想法) the way Zhihu's editors do: both go
//! back through `content/publish` as published content
//! (`draft.isPublished`). An answer names itself in `draft.contentId`
//! (the answer editor's `w`, account.app bundle) and keeps its settings:
//! reposting and comments are left out, the reward, disclaimer, thanks
//! and content source go back as `answers/{id}` and its `settings`
//! report them. A pin names itself in `draft.id` (the
//! `editPinV2RichInput` chunk) and is loaded from `content/plugin`, the
//! editor's own data, so its pictures, video, link card, topics, circle
//! and place go back as they were. Articles, questions and videos are
//! not edited.

use atp_core::cli::Change;
use atp_core::text::html_to_text;
use atp_core::{Action, Edit, Error, ErrorCode, Result, Value, ValueExt, json};

use super::{AnswerForm, answer_data, content_publish, paragraphs, trace_id};
use crate::api::{Api, V4, WWW};
use crate::refs::{Target, answer_url};

const ANSWER_INCLUDE: &str = "content,editable_content,reward_info,attachment,content_source,question,answer_type,paid_info,relationship.is_author";
/// Parts of a pin's editor data that go back as they are.
const PIN_KEPT: &[&str] = &["linkCard", "topic", "ring", "location"];

/// Change one of the account's answers or pins.
pub async fn edit(api: &Api, target: &Target, edit: &Edit) -> Result<Action> {
  match target {
    Target::Answer(id) => answer(api, id, edit).await,
    Target::Pin(id) => pin(api, id, edit).await,
    Target::Article(_) => Err(
      Error::unsupported("edit (articles)").with_hint("change articles in Zhihu's column editor"),
    ),
    Target::Question(_) => Err(Error::unsupported("edit (questions)")),
    Target::Zvideo(_) => Err(Error::unsupported("edit (videos)")),
  }
}

async fn answer(api: &Api, id: &str, edit: &Edit) -> Result<Action> {
  if edit.title.is_some() {
    return Err(Error::input(
      "an answer has no title; its question's is not part of it",
    ));
  }
  let Some(text) = edit.text.as_deref() else {
    return Err(Error::input("give the answer's new text"));
  };
  let url = format!("{V4}/answers/{id}");
  let v = api
    .call(api.get(&url).query("include", ANSWER_INCLUDE))
    .await?;
  refuse(&v)?;
  let settings = api.call(api.get(&format!("{url}/settings"))).await?;
  let question = v
    .str("question.id")
    .ok_or_else(|| Error::upstream("Zhihu did not say which question the answer is under"))?;
  let old = v
    .first_str(&["editable_content", "content"])
    .unwrap_or_default();
  let mut change = Change::new(format!("Zhihu answer {id}:"));
  change.field("text", &html_to_text(&old), text);
  let pictures = old.matches("<img").count();
  if pictures > 0 {
    change.field("pictures", &pictures.to_string(), "0");
  }
  change.confirm(edit.yes, "change this answer on Zhihu")?;
  api.prepare_write().await?;
  let draft = json!({ "contentId": id, "isPublished": true, "disabled": 1 });
  let form = form(&v, &settings);
  let data = answer_data(
    &question,
    paragraphs(text),
    text.chars().count(),
    &form,
    draft,
  );
  content_publish(api, &json!({ "action": "answer", "data": data })).await?;
  Ok(
    Action::done("edit", id)
      .with_id(id)
      .with_url(answer_url(id, Some(&question))),
  )
}

/// Answers whose editor sends what ATP does not: someone else's, video
/// answers and paid ones.
fn refuse(v: &Value) -> Result<()> {
  if v.bool("relationship.is_author") == Some(false) {
    return Err(Error::new(
      ErrorCode::PermissionDenied,
      "this answer is not yours",
    ));
  }
  let video = matches!(
    v.str("attachment.type").as_deref(),
    Some("video" | "zvideo")
  );
  let paid = v.str("answer_type").as_deref() == Some("paid_column_content")
    || v.at("paid_info").as_object().is_some_and(|o| !o.is_empty());
  match (video, paid) {
    (true, _) => Err(Error::unsupported("edit of video answers")),
    (_, true) => Err(Error::unsupported("edit of paid answers")),
    _ => Ok(()),
  }
}

/// The answer's settings as its editor fills them in when it reopens it.
fn form(answer: &Value, settings: &Value) -> AnswerForm {
  let tagline = answer.str("reward_info.tagline").unwrap_or_default();
  let disclaimer = settings.at("disclaimer_info.0");
  let open = disclaimer.str("status").as_deref() == Some("open");
  let source = answer.at("content_source");
  let declared = source.str("channel").is_some() || source.str("display_str").is_some();
  let thanked = settings.str("thank_inviter_status").as_deref() == Some("open");
  AnswerForm {
    reshipment: None,
    comments: None,
    can_reward: answer.bool("reward_info.is_rewardable") == Some(true) || !tagline.is_empty(),
    tagline,
    disclaimer_status: if open { "open" } else { "close" }.into(),
    disclaimer_type: disclaimer
      .str("type")
      .filter(|_| open)
      .unwrap_or_else(|| "none".into()),
    thank_inviter: settings
      .first_str(&["thank_inviter.id", "thank_inviter"])
      .filter(|_| thanked)
      .unwrap_or_default(),
    is_report: settings.bool("commercial_report_info.is_report") == Some(true),
    content_source: if declared {
      source.clone()
    } else {
      Value::Null
    },
  }
}

async fn pin(api: &Api, id: &str, edit: &Edit) -> Result<Action> {
  let plugin = plugin(api, id).await?;
  if !plugin.at("vote").is_null() {
    return Err(Error::unsupported("edit of pins with a vote"));
  }
  let old_html = plugin.str("hybrid.html").unwrap_or_default();
  let old_title = plugin.str("title.title").unwrap_or_default();
  let title = edit
    .title
    .as_deref()
    .map_or_else(|| old_title.clone(), |t| t.trim().to_owned());
  let (html, text_len) = match &edit.text {
    Some(text) => (paragraphs(text), text.chars().count()),
    None => {
      let len = html_to_text(&old_html).chars().count();
      let len = plugin.u64("hybrid.textLength").map_or(len, |n| n as usize);
      (old_html.clone(), len)
    }
  };
  let medias: Vec<Value> = plugin
    .list("media.medias")
    .iter()
    .filter(|m| m.get("image").is_some() || m.get("video").is_some())
    .cloned()
    .collect();
  if html.is_empty() && medias.is_empty() {
    return Err(Error::input("a pin without pictures needs a text"));
  }
  let mut change = Change::new(format!("Zhihu pin {id}:"));
  change.field("title", &old_title, &title).field(
    "text",
    &html_to_text(&old_html),
    &html_to_text(&html),
  );
  if !medias.is_empty() {
    change.row("media", &format!("{} (kept)", medias.len()));
  }
  change.confirm(edit.yes, "change this pin on Zhihu")?;
  api.prepare_write().await?;
  let data = pin_data(id, &plugin, &title, (html, text_len), medias);
  content_publish(api, &json!({ "action": "pin", "data": data })).await?;
  Ok(
    Action::done("edit", id)
      .with_id(id)
      .with_url(format!("{WWW}/pin/{id}")),
  )
}

/// The pin editor's data of the published pin (`onlinePlugin`), the way
/// `content/publish` took it.
async fn plugin(api: &Api, id: &str) -> Result<Value> {
  let v = api
    .call(
      api
        .get(&format!("{WWW}/content/plugin/{id}"))
        .query("action", "pin"),
    )
    .await?;
  let plugin = match v.at("data.onlinePlugin") {
    Value::String(text) => serde_json::from_str(text)
      .map_err(|e| Error::upstream(format!("unreadable pin data from Zhihu's editor: {e}")))?,
    Value::Object(_) => v.at("data.onlinePlugin").clone(),
    _ => return Err(Error::not_found(format!("no pin {id} to edit"))),
  };
  if plugin.at("hybrid").is_null() && plugin.at("media").is_null() {
    return Err(Error::upstream(
      "Zhihu's pin editor sent data ATP does not know; nothing was changed",
    ));
  }
  Ok(plugin)
}

/// The `data` of `content/publish` for a changed pin (the editor's `tc`).
fn pin_data(
  id: &str,
  plugin: &Value,
  title: &str,
  body: (String, usize),
  medias: Vec<Value>,
) -> Value {
  let permission = plugin
    .first_str(&[
      "commentsPermission.comment_permission",
      "commentsPermission.commentPermission",
    ])
    .unwrap_or_else(|| "all".into());
  let mut data = json!({
    "draft": { "disabled": 1, "id": id, "isPublished": true },
    "publish": { "traceId": trace_id() },
    "commentsPermission": { "comment_permission": permission },
    "extra_info": { "view_permission": "all", "publisher": "pc" },
  });
  if !title.is_empty() {
    data["title"] = json!({ "title": title });
  }
  let (html, text_len) = body;
  if !html.is_empty() {
    data["hybrid"] = json!({ "html": html, "textLength": text_len });
  }
  if !medias.is_empty() {
    data["media"] = json!({ "medias": medias });
  }
  for key in PIN_KEPT {
    let part = plugin.at(key);
    if part.as_object().is_some_and(|o| !o.is_empty()) {
      data[*key] = part.clone();
    }
  }
  data
}
