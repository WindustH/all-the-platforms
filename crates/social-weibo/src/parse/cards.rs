//! m.weibo.cn's card containers of search and topic pages.

use super::*;

/// All posts inside m.weibo.cn cards, including those nested in card groups.
pub fn card_posts(cards: &[Value]) -> Vec<Post> {
  let mut out = Vec::new();
  for card in cards {
    if card.i64("card_type") == Some(9) {
      out.extend(post(card.at("mblog")));
    }
    out.extend(card_posts(card.list("card_group")));
  }
  out
}

/// All users inside m.weibo.cn cards.
pub fn card_users(cards: &[Value]) -> Vec<User> {
  let mut out = Vec::new();
  for card in cards {
    if card.at("user").is_object() && card.i64("card_type") != Some(9) {
      out.extend(user(card.at("user")));
    }
    out.extend(card_users(card.list("card_group")));
  }
  out
}

pub fn card_topics(cards: &[Value]) -> Vec<Collection> {
  let mut out = Vec::new();
  for card in cards {
    if card.i64("card_type") == Some(8) {
      out.extend(topic(card));
    }
    out.extend(card_topics(card.list("card_group")));
  }
  out
}
