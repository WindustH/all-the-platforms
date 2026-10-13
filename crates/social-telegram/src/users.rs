//! The account and other users, on whichever route is active.

use social_core::{Result, User};

use crate::api::{Api, Mode};
use crate::refs::{self, Chat};
use crate::{mt, web};

pub async fn whoami(api: &Api) -> Result<User> {
  match api.mode()? {
    Mode::User(_) => api.as_user(async |c| mt::whoami(c).await).await,
    Mode::Bot(b) => b.whoami(&api.ctx).await,
    Mode::Web => Err(api.needs_account("whoami")),
  }
}

pub async fn user(api: &Api, user: &str) -> Result<User> {
  match api.mode()? {
    Mode::User(_) => api.as_user(async |c| mt::user(c, user).await).await,
    Mode::Bot(b) => b.user(&api.ctx, user).await,
    Mode::Web => match refs::chat(user)? {
      Chat::Username(u) => web::profile(&api.ctx, &u).await,
      _ => Err(api.needs_account("looking up ids")),
    },
  }
}
