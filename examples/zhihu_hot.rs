//! Zhihu's hot list and a user's posts, with the session `atp zhihu login`
//! saved (the hot list works without one).
//!
//!     cargo run --example zhihu_hot --no-default-features --features zhihu

use atp::zhihu::Zhihu;
use atp::{PageReq, Platform};

#[tokio::main(flavor = "current_thread")]
async fn main() -> atp::Result<()> {
  let zhihu: Zhihu = atp::open()?;
  let hot = zhihu.hot(None, &PageReq::first(10)).await?;
  for post in &hot.items {
    println!("{} {}", post.id, post.title.as_deref().unwrap_or_default());
  }
  // Up to 100 posts of a user, page after page.
  let all = atp::paging::collect(100, None, async |page| {
    zhihu.user_posts("zhihuadmin", &page).await
  })
  .await?;
  println!("{} posts", all.items.len());
  Ok(())
}
