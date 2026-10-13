//! The bot's channel: message quota, webhook and rich menus.

use social_core::{Result, Value, ValueExt, json};

use crate::api::{API, Api};

/// Monthly message quota and what was used.
pub async fn quota(api: &Api) -> Result<Value> {
  let q = api.get("/v2/bot/message/quota").await?;
  let used = api.get("/v2/bot/message/quota/consumption").await?;
  let webhook = api
    .get(&format!("{API}/v2/bot/channel/webhook/endpoint"))
    .await
    .unwrap_or_default();
  Ok(json!({
    "type": q.str("type"),
    "limit": q.u64("value"),
    "used": used.u64("totalUsage"),
    "webhook": webhook.str("endpoint"),
    "webhook_active": webhook.bool("active"),
  }))
}

pub async fn rich_menus(api: &Api) -> Result<Value> {
  let list = api.get("/v2/bot/richmenu/list").await?;
  let default = api
    .get("/v2/bot/user/all/richmenu")
    .await
    .ok()
    .and_then(|d| d.str("richMenuId"));
  Ok(json!({"default": default, "richmenus": list.at("richmenus")}))
}
