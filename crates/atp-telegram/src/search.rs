//! Searching messages and users: user accounts only.

use atp_core::{Page, PageReq, Post, Query, Result, User};

use crate::api::{Api, Mode, bots_cannot};
use crate::mt;

pub async fn posts(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  match api.mode()? {
    Mode::User(_) => {
      let filter = query.filter.as_deref();
      api
        .as_user(async |c| mt::search(c, &query.keyword, filter, page).await)
        .await
    }
    Mode::Bot(_) => Err(bots_cannot("search")),
    Mode::Web => Err(api.needs_account("searching all chats").with_hint(
      "without an account, `atp telegram search-chat @channel WORDS` searches a public channel",
    )),
  }
}

pub async fn users(api: &Api, query: &Query, page: &PageReq) -> Result<Page<User>> {
  match api.mode()? {
    Mode::User(_) => {
      api
        .as_user(async |c| mt::search_users(c, &query.keyword, page).await)
        .await
    }
    Mode::Bot(_) => Err(bots_cannot("search --type user")),
    Mode::Web => Err(api.needs_account("searching users")),
  }
}
