//! The account: its name from the account page, its member type and
//! storage from `/member`.

use atp_core::text::fmt_size;
use atp_core::{Data, Result, User, Value, ValueExt, json};

use crate::api::Api;
use crate::parse;

pub async fn whoami(api: &Api) -> Result<User> {
  api.ctx.require_login(&["__pus"])?;
  let account = api.account().await?;
  let member = member(api).await?;
  Ok(parse::user(&account, &member))
}

/// Storage used and left, and the member type.
pub async fn quota(api: &Api) -> Result<Data> {
  api.ctx.require_login(&["__pus"])?;
  let member = member(api).await?;
  let total = member.u64("total_capacity").unwrap_or(0);
  let used = member.u64("use_capacity").unwrap_or(0);
  let free = total.saturating_sub(used);
  let mut out = json!({
    "member": parse::member_label(&member.str("member_type").unwrap_or_default()),
    "total_bytes": total,
    "used_bytes": used,
    "free_bytes": free,
    "total": fmt_size(total),
    "used": fmt_size(used),
    "free": fmt_size(free),
  });
  if let Some(at) = parse::time(&member, &["super_vip_exp_at", "exp_at"]) {
    out["member_until"] = json!(at);
  }
  Ok(Data::Value(out))
}

async fn member(api: &Api) -> Result<Value> {
  let query = [
    ("fetch_subscribe", "true".to_owned()),
    ("_ch", "home".into()),
    ("fetch_identity", "true".into()),
  ];
  Ok(api.get("/member", &query).await?.at("data").clone())
}
