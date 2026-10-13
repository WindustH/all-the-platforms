//! Insights of a business or creator account through the Graph API: an
//! Instagram-login token (`IG…`, graph.instagram.com) or a Facebook token of
//! the Page the account is linked to (graph.facebook.com). Asked one metric
//! at a time, since Meta retires names and one unknown name fails a request.

mod account;
mod post;

use atp_core::{Error, Insights, Result, ValueExt, credentials};
use atp_meta::Graph;
use atp_meta::graph::{fields, host_for};

use crate::api::Api;

const HINT: &str = "set INSTAGRAM_ACCESS_TOKEN (an Instagram-login token, or a Facebook token of the \
   linked Page with instagram_basic and instagram_manage_insights) or run \
   `atp instagram login --token <token>`; INSTAGRAM_USER_ID picks the account";

/// The account's metrics over the last `days` (at most 30).
pub async fn account(api: &Api, days: u32) -> Result<Insights> {
  let (graph, account) = graph_account(api).await?;
  account::insights(&graph, &account, days).await
}

/// Lifetime metrics of one of the account's posts.
pub async fn post(api: &Api, post: &str) -> Result<Insights> {
  let (graph, account) = graph_account(api).await?;
  post::insights(&graph, &account, post).await
}

/// The Graph API and the Instagram account id to ask about:
/// INSTAGRAM_USER_ID, the Instagram-login token's own, or the account
/// linked to one of the token's Pages.
async fn graph_account(api: &Api) -> Result<(Graph<'_>, String)> {
  let token = api
    .token()
    .ok_or_else(|| Error::auth("no Graph API token for Instagram insights").with_hint(HINT))?;
  let ig_login = token.starts_with("IG");
  let graph = Graph::new(&api.ctx, host_for(&token), token);
  if let Some(id) = credentials::env("INSTAGRAM_USER_ID") {
    return Ok((graph, id));
  }
  if ig_login {
    let v = graph.get("me", &fields("user_id,username")).await?;
    let id = v
      .str("user_id")
      .or_else(|| v.str("id"))
      .ok_or_else(|| Error::upstream("the token names no Instagram account"))?;
    return Ok((graph, id));
  }
  let pages = graph
    .all(
      "me/accounts",
      &fields("instagram_business_account{id,username}"),
      0,
    )
    .await?;
  let id = pages
    .iter()
    .find_map(|page| page.str("instagram_business_account.id"))
    .ok_or_else(|| {
      Error::not_found("no Instagram business account linked to the token's Pages").with_hint(HINT)
    })?;
  Ok((graph, id))
}
