//! Podcast Index keys (`login --token KEY:SECRET`). They belong to no
//! account: a signed request that goes through shows they work.

use atp_core::{Error, Result, User, credentials};

use crate::api::{self, Api};

/// Keep the key and secret; `whoami` checks them right after.
pub fn token(api: &Api, token: &str) -> Result<()> {
  let fields = credentials::expect(token, 2, 2, "KEY:SECRET")?;
  credentials::save(
    &api.ctx,
    &[(api::KEY, &fields[0]), (api::SECRET, &fields[1])],
  );
  Ok(())
}

pub async fn whoami(api: &Api) -> Result<User> {
  let (key, _) = api.index_keys().ok_or_else(|| {
    Error::auth("no Podcast Index key").with_hint("`atp podcast login --token KEY:SECRET`")
  })?;
  api.index("stats/current", Vec::new()).await?;
  Ok(User {
    id: credentials::mask(&key),
    name: "Podcast Index API key".into(),
    ..User::default()
  })
}
