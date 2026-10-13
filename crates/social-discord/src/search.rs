//! Discord's message search (user accounts only), scoped by tokens in the
//! query like the app's search box: `server:ID` (or `guild:`), `in:CHANNEL`,
//! `from:USER`, `mentions:USER`, `has:link|embed|file|image|video|sound|sticker|poll`,
//! `pinned:true`, `before:` / `after:` (dates or durations like `7d`). The
//! rest of the words are the text to find. `--filter` is `has:`, `--sort`
//! `newest` (default), `oldest` or `relevance`.

use social_core::text::parse_when;
use social_core::{Error, Page, PageReq, Post, Query, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::{chats, parse, refs};

pub const SORTS: &[&str] = &["newest", "oldest", "relevance"];
pub const HAS: &[&str] = &[
  "link", "embed", "file", "image", "video", "sound", "sticker", "poll",
];
/// Results per search page (fixed by Discord).
const PAGE: u64 = 25;
/// Discord stops paging past this offset.
const MAX_OFFSET: u64 = 9975;

#[derive(Default)]
struct Scope {
  server: Option<String>,
  channel: Option<String>,
  params: Vec<(&'static str, String)>,
  words: Vec<String>,
}

fn scope(q: &Query) -> Result<Scope> {
  let mut s = Scope::default();
  for word in q.keyword.split_whitespace() {
    let Some((key, value)) = word.split_once(':').filter(|(k, v)| {
      !v.is_empty()
        && matches!(
          *k,
          "server" | "guild" | "in" | "from" | "mentions" | "has" | "pinned" | "before" | "after"
        )
    }) else {
      s.words.push(word.to_owned());
      continue;
    };
    match key {
      "server" | "guild" => s.server = Some(refs::chat(value).map(id_of)?),
      "in" => s.channel = Some(refs::chat(value).map(id_of)?),
      "from" => s.params.push(("author_id", refs::user(value)?)),
      "mentions" => s.params.push(("mentions", refs::user(value)?)),
      "has" => s.params.push(("has", value.to_owned())),
      "pinned" => s.params.push(("pinned", value.to_owned())),
      "before" | "after" => {
        let at = parse_when(value).map_err(Error::input)?;
        let name = if key == "before" { "max_id" } else { "min_id" };
        s.params.push((name, parse::snowflake_at(at)));
      }
      _ => unreachable!(),
    }
  }
  if let Some(has) = &q.filter {
    s.params.push(("has", has.clone()));
  }
  Ok(s)
}

fn id_of(c: refs::Chat) -> String {
  match c {
    refs::Chat::Id(id)
    | refs::Chat::Channel(id)
    | refs::Chat::Server(id)
    | refs::Chat::User(id) => id,
  }
}

pub async fn messages(api: &Api, q: &Query, page: &PageReq) -> Result<Page<Post>> {
  api.require_user("search (bots cannot search messages)")?;
  let s = scope(q)?;
  let (path, mut params) = match (&s.server, &s.channel) {
    (Some(server), channel) => {
      let mut params = Vec::new();
      if let Some(c) = channel {
        params.push(("channel_id", c.clone()));
      }
      (format!("/guilds/{server}/messages/search"), params)
    }
    (None, Some(channel)) => {
      let info = api.channel(channel).await?;
      if info.guild == "@me" {
        (format!("/channels/{channel}/messages/search"), Vec::new())
      } else {
        (
          format!("/guilds/{}/messages/search", info.guild),
          vec![("channel_id", channel.clone())],
        )
      }
    }
    (None, None) => {
      return Err(
        Error::input("say where to search: add `server:ID` or `in:CHANNEL` to the query")
          .with_hint("`social discord chats` lists servers and chats; e.g. `social discord search 'rust server:123'`"),
      );
    }
  };
  let words = s.words.join(" ");
  if !words.is_empty() {
    params.push(("content", words));
  }
  params.extend(s.params);
  let (by, order) = match q.sort.as_deref() {
    Some("oldest") => ("timestamp", "asc"),
    Some("relevance") => ("relevance", "desc"),
    _ => ("timestamp", "desc"),
  };
  let offset = page.number_or(0);
  params.extend([
    ("sort_by", by.to_owned()),
    ("sort_order", order.to_owned()),
    ("include_nsfw", "true".to_owned()),
    ("offset", offset.to_string()),
  ]);
  let v = api.get(&path, &params).await?;
  let total = v.u64("total_results").unwrap_or_default();
  let items: Vec<Post> = v
    .list("messages")
    .iter()
    .filter_map(|group| hit(group))
    .map(|m| {
      let name = m.str("channel_id").and_then(|c| channel_name(&v, &c));
      parse::message(m, s.server.as_deref(), name.as_deref())
    })
    .collect();
  let next = offset + PAGE;
  let more = !items.is_empty() && next < total && next <= MAX_OFFSET;
  Ok(Page::new(items, more.then(|| next.to_string())))
}

/// The matching message of a result group (older answers hold context too).
fn hit(group: &Value) -> Option<&Value> {
  match group.as_array() {
    Some(rows) => rows
      .iter()
      .find(|m| m.bool("hit") == Some(true))
      .or_else(|| rows.first()),
    None => Some(group),
  }
}

/// Channel names come along in the answer's `threads` / `channels` lists, when at all.
fn channel_name(v: &Value, id: &str) -> Option<String> {
  ["threads", "channels"]
    .iter()
    .flat_map(|k| v.list(k))
    .find(|c| c.str("id").as_deref() == Some(id))
    .map(parse::chat_name)
}

/// `search -t user`: members of a server whose name starts with the query
/// (`server:ID` in the query).
pub async fn users(api: &Api, q: &Query, page: &PageReq) -> Result<Page<User>> {
  let s = scope(q)?;
  let server = match (s.server, s.channel) {
    (Some(server), _) => server,
    (None, Some(channel)) => api.channel(&channel).await?.guild,
    (None, None) => {
      return Err(Error::input(
        "say which server: add `server:ID` to the query (Discord has no global user search)",
      ));
    }
  };
  chats::search_members(api, &server, &s.words.join(" "), page).await
}
