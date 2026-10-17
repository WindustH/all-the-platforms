//! Your own following as the relation page (我的关注) shows it: sorted,
//! searched or filtered through `followContent`, and the special-follow
//! list (特别关注).

use atp_core::{Error, FollowQuery, Page, PageReq, Result, User, ValueExt};

use crate::api::Api;
use crate::parse::{self, Own};
use crate::users;

/// `following --sort` values: `recent` (最新关注, latest follow first) and
/// `posted` (最新发博, latest post first; only accounts that posted in
/// the last seven days).
pub const SORTS: &[&str] = &["recent", "posted"];

/// A `following-filter`: when you followed them, or that they went quiet.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum Filter {
  /// Followed in the last 7 days
  #[value(name = "7d")]
  Week,
  /// Followed in the last 30 days
  #[value(name = "30d")]
  Month,
  /// Followed in the last half year
  #[value(name = "180d")]
  HalfYear,
}

impl Filter {
  /// The page's `filter_code`.
  fn code(self) -> &'static str {
    match self {
      Filter::Week => "1",
      Filter::Month => "2",
      Filter::HalfYear => "3",
    }
  }
}

/// `filter_code` 5: no posts in the last 30 days (近30天未发博).
const INACTIVE: &str = "5";

/// `following` with `--sort` or `--search`. They read the relation page,
/// which only lists your own following; `--sort recent` of someone else
/// is their plain list.
pub async fn following(
  api: &Api,
  input: &str,
  query: &FollowQuery,
  page: &PageReq,
) -> Result<Page<User>> {
  if query.mutual {
    return Err(Error::unsupported("following --mutual"));
  }
  let own = users::uid(api, input).await? == users::own_uid(api).await?;
  match (query.search.as_deref(), query.sort.as_deref()) {
    (Some(_), Some(_)) => Err(Error::input(
      "--search lists matches in follow order; drop --sort",
    )),
    (None, None | Some("recent")) if !own => users::following(api, input, page).await,
    _ if !own => Err(Error::input(
      "--sort posted and --search work on your own following only",
    )),
    (Some(words), None) => content(api, &[("sortType", "search"), ("q", words)], page).await,
    (None, None | Some("recent")) => content(api, &[("sortType", "")], page).await,
    (None, Some("posted")) => content(api, &[("sortType", "timeDown")], page).await,
    (None, Some(other)) => Err(Error::input(format!(
      "unknown --sort {other} (recent or posted)"
    ))),
  }
}

/// Your following narrowed by when you followed them, or (`inactive`) to
/// the accounts that posted nothing in the last 30 days.
pub async fn filtered(
  api: &Api,
  within: Option<Filter>,
  inactive: bool,
  page: &PageReq,
) -> Result<Page<User>> {
  let code = match (within, inactive) {
    (Some(f), false) => f.code(),
    (None, true) => INACTIVE,
    _ => return Err(Error::input("give either --within or --inactive")),
  };
  content(api, &[("sortType", "filter"), ("filter_code", code)], page).await
}

/// One page of `followContent`. The cursor is `PAGE:NEXT_CURSOR`: the page
/// counts up and `next_cursor` is passed back, except for `timeDown`,
/// which the page pages by `next_cursor` alone.
async fn content(api: &Api, params: &[(&str, &str)], page: &PageReq) -> Result<Page<User>> {
  api.require_login()?;
  let cursor = page.cursor.clone().unwrap_or_default();
  let (no, next) = cursor.split_once(':').unwrap_or(("1", ""));
  let no: u64 = no.parse().unwrap_or(1);
  let no_text = no.to_string();
  let mut query = params.to_vec();
  query.extend([("page", no_text.as_str()), ("next_cursor", next)]);
  let v = api.get("/ajax/profile/followContent", &query).await?;
  let follows = v.at("data.follows");
  let users: Vec<User> = follows
    .list("users")
    .iter()
    .filter_map(|u| parse::relation(u, Own::Following))
    .collect();
  let next = follows.str("next_cursor").filter(|n| n != "0");
  let step = if params.contains(&("sortType", "timeDown")) {
    0
  } else {
    1
  };
  let cursor = next
    .filter(|_| !users.is_empty())
    .map(|n| format!("{}:{n}", no + step));
  Ok(Page::new(users, cursor))
}

/// Your special follows (特别关注), 20 a page.
pub async fn special(api: &Api, page: &PageReq) -> Result<Page<User>> {
  let uid = users::own_uid(api).await?;
  let no = page.number_or(1);
  let no_text = no.to_string();
  let v = api
    .get(
      "/ajax/profile/getSpecialAttention",
      &[
        ("page", no_text.as_str()),
        ("count", "20"),
        ("uid", &uid),
        ("type", "special_attention"),
        ("relate", "fans"),
      ],
    )
    .await?;
  let users: Vec<User> = v
    .list("special_attention.list")
    .iter()
    .filter_map(|u| parse::relation(u, Own::Following))
    .collect();
  let next = (!users.is_empty()).then(|| (no + 1).to_string());
  Ok(Page::new(users, next))
}
