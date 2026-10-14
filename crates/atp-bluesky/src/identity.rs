//! Who is who: handles → DIDs (`com.atproto.identity.resolveHandle` on
//! the public AppView, cached for a day), DID documents (`plc.directory`
//! for `did:plc`, `/.well-known/did.json` for `did:web`), and the PDS a
//! DID document names.

use std::time::Duration;

use atp_core::http::{Method, TOOL_USER_AGENT};
use atp_core::{Error, Result, Value, ValueExt};

use crate::api::{self, Api};

const PLC: &str = "https://plc.directory";
const DID_TTL: Duration = Duration::from_secs(24 * 3600);

/// The DID of a handle (a DID stays as it is).
pub async fn did(api: &Api, actor: &str) -> Result<String> {
  let actor = actor.trim().trim_start_matches('@').to_ascii_lowercase();
  if actor.starts_with("did:") {
    return Ok(actor);
  }
  let key = format!("did-{actor}");
  if let Some(did) = api.ctx.store.cache_get::<String>(&key, DID_TTL) {
    return Ok(did);
  }
  let query = vec![("handle", actor.clone())];
  let resp = api
    .send(
      Method::GET,
      api::PUBLIC,
      "com.atproto.identity.resolveHandle",
      query,
      None,
      None,
    )
    .await?;
  let did = api::check(&resp)
    .map_err(|e| Error::not_found(format!("no Bluesky account @{actor} ({})", e.message)))?
    .str("did")
    .ok_or_else(|| Error::not_found(format!("no Bluesky account @{actor}")))?;
  api.ctx.store.cache_put(&key, &did);
  Ok(did)
}

/// The DID document of a DID.
pub async fn document(api: &Api, did: &str) -> Result<Value> {
  let url = match did.strip_prefix("did:web:") {
    Some(host) => format!("https://{host}/.well-known/did.json"),
    None => format!("{PLC}/{did}"),
  };
  api
    .ctx
    .http
    .get(url)
    .no_cookies()
    .header("user-agent", TOOL_USER_AGENT)
    .value()
    .await
    .map_err(|e| Error::not_found(format!("no DID document for {did}: {}", e.message)))
}

/// The PDS a DID document names (`#atproto_pds`).
pub fn pds_in(doc: &Value) -> Option<String> {
  doc
    .list("service")
    .iter()
    .find(|s| s.str("id").is_some_and(|id| id.ends_with("#atproto_pds")))
    .and_then(|s| s.str("serviceEndpoint"))
    .map(|url| url.trim_end_matches('/').to_owned())
}

/// The PDS that keeps an account's repository.
pub async fn pds(api: &Api, actor: &str) -> Result<String> {
  let did = did(api, actor).await?;
  let doc = document(api, &did).await?;
  pds_in(&doc).ok_or_else(|| Error::upstream(format!("the DID document of {did} names no PDS")))
}
