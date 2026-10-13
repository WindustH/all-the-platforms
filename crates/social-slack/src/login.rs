//! `login --token`: a bot (`xoxb-`) or user (`xoxp-`) token, optionally
//! with an app-level `xapp-` token for Socket Mode, checked with `auth.test`.

use social_core::{Error, Result};

use crate::api::{self, Api, Identity};

/// Check a token (and an optional app-level token) and keep them.
pub async fn login(api: &Api, raw: &str) -> Result<()> {
  let mut token = None;
  let mut app = None;
  for part in raw.split(|c: char| c == '|' || c.is_whitespace()) {
    let part = part.trim();
    if part.starts_with("xapp-") {
      app = Some(part.to_owned());
    } else if part.starts_with("xox") {
      token = Some(part.to_owned());
    } else if !part.is_empty() {
      return Err(
        Error::input("that is not a Slack token")
          .with_hint("expected xoxb-… (bot) or xoxp-… (user), optionally with an xapp-… app token"),
      );
    }
  }
  let Some(token) = token else {
    return Err(Error::input("no xoxb-… or xoxp-… token given").with_hint(
      "an app-level xapp-… token alone cannot read anything; pass it together with the bot or user token",
    ));
  };
  if token.starts_with("xoxc-") {
    return Err(
      Error::unsupported("xoxc- browser tokens")
        .with_hint("create a Slack app (api.slack.com/apps) and use its bot or user OAuth token"),
    );
  }
  let id: Identity = api::identity(&api.call_with(&token, "auth.test", Vec::new()).await?)?;
  api.save(&token, app.as_deref(), &id);
  Ok(())
}
