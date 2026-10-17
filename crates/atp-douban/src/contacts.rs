//! Following people (关注), blocking them (加入黑名单) and remarks (备注),
//! as a person's page on www.douban.com does them: form posts to
//! `/j/contact/…` with the person's number and `ck`, which answer
//! `{result, msg}`.

use atp_core::bulk;
use atp_core::{Action, Error, ErrorCode, Result, Value, ValueExt};

use crate::api::{Api, WWW};
use crate::{refs, users};

/// A person's number, which the contact calls take, and their page.
async fn person(api: &Api, arg: &str) -> Result<(String, String)> {
  let name = refs::user(arg)?;
  let id = if name.bytes().all(|b| b.is_ascii_digit()) {
    name.clone()
  } else {
    users::user(api, &name).await?.id
  };
  Ok((id, format!("/people/{name}/")))
}

/// The answer of a contact call, failing unless its `result` is true.
fn accepted(v: Option<Value>, page: &str) -> Result<Value> {
  let Some(v) = v else {
    return Err(Error::upstream("Douban answered without a result"));
  };
  if v.bool("result") == Some(true) {
    return Ok(v);
  }
  let msg = v.str("msg").unwrap_or_else(|| "refused".into());
  Err(match msg.as_str() {
    "need_captcha" => Error::verification("Douban wants a captcha before this follow")
      .with_url(format!("{WWW}{page}"))
      .with_hint("follow on the page once, then slow down")
      .unclearable(),
    "readonly" => Error::new(
      ErrorCode::PermissionDenied,
      "Douban has made this account read-only (帐号异常)",
    ),
    _ => Error::upstream(format!("Douban: {msg}")),
  })
}

pub async fn follow(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let (id, page) = person(api, arg).await?;
  let form = [("people", id.clone())];
  if undo {
    // The page reloads after this call whatever it answers.
    let v = api
      .post_www("/j/contact/removecontact", &page, &form)
      .await?;
    if let Some(v) = v.filter(|v| v.bool("result") == Some(false)) {
      accepted(Some(v), &page)?;
    }
  } else {
    let v = api.post_www("/j/contact/addcontact", &page, &form).await?;
    accepted(v, &page)?;
  }
  let action = if undo { "unfollow" } else { "follow" };
  Ok(Action::done(action, id).with_url(format!("{WWW}{page}")))
}

/// Put users on your blacklist, one call each. Douban's web has no call
/// to take someone off it.
pub async fn block(api: &Api, inputs: &[String], undo: bool) -> Result<Vec<Action>> {
  if undo {
    return Err(
      Error::unsupported("unblock").with_hint("take people off the blacklist on douban.com itself"),
    );
  }
  bulk::each("block", inputs, async |input| {
    let (id, page) = person(api, input).await?;
    let v = api
      .post_www(
        "/j/contact/addtoblacklist",
        &page,
        &[("people", id.clone())],
      )
      .await?;
    accepted(v, &page)?;
    Ok(Action::done("block", id).with_url(format!("{WWW}{page}")))
  })
  .await
}

/// Set the remark you see for someone you follow; empty clears it.
pub async fn remark(api: &Api, arg: &str, text: &str) -> Result<Action> {
  let (id, page) = person(api, arg).await?;
  let form = [("people", id.clone()), ("remark", text.to_owned())];
  let v = accepted(
    api.post_www("/j/contact/remark", &page, &form).await?,
    &page,
  )?;
  let message = match v.str("data") {
    Some(remark) => format!("remark {remark}"),
    None => "remark cleared".to_owned(),
  };
  Ok(
    Action::done("remark", id)
      .with_url(format!("{WWW}{page}"))
      .with_message(message),
  )
}
