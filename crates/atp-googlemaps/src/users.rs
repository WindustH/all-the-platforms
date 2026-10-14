//! `whoami`: a key has no account behind it, so this checks the key with
//! an IDs-only Text Search, which Google does not bill, and describes it.

use atp_core::credentials;
use atp_core::output::note;
use atp_core::{ErrorCode, Result, User};

use crate::api::Api;
use crate::places;

pub async fn whoami(api: &Api) -> Result<User> {
  let key = api.require()?;
  let places = match places::check(api).await {
    Ok(()) => true,
    // The key is valid; only Places API (New) is off for it.
    Err(e) if e.code == ErrorCode::PermissionDenied => {
      note(&format!("{} ({})", e.message, e.hint.unwrap_or_default()));
      false
    }
    Err(e) => return Err(e),
  };
  let mut user = User {
    id: credentials::mask(&key),
    name: "Google Maps API key".into(),
    url: Some("https://console.cloud.google.com/google/maps-apis/credentials".into()),
    ..User::default()
  };
  user.extra.insert("places_api".into(), places.into());
  if let Some(language) = &api.language {
    user
      .extra
      .insert("language".into(), language.clone().into());
  }
  Ok(user)
}
