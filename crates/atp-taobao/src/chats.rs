//! `chats`: the account's chats with shops, the most recently active first.

use atp_alibaba::chats::chat;
use atp_core::{Collection, Page, PageReq, Result};

use crate::api::Api;

pub async fn list(api: &Api, page: &PageReq) -> Result<Page<Collection>> {
  let me = api.me()?;
  let mut im = api.im().await?;
  let batch = im
    .conversations(page.cursor.as_deref(), page.size_within(50))
    .await?;
  let items = batch.items.iter().filter_map(|c| chat(c, &me)).collect();
  Ok(Page::new(items, batch.next))
}
