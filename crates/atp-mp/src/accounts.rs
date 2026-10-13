//! Accounts: yours, or another one by link, `__biz` / fakeid or name
//! (searched through the backend).

use atp_core::{Error, Page, PageReq, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::refs;

/// Whose articles: the logged-in account, or another one by fakeid.
pub enum Account {
  Own,
  Other {
    fakeid: String,
    user: Option<Box<User>>,
  },
}

/// Resolve an account argument: empty or your own `gh_` id, a fakeid
/// (`__biz`), an article or album link, or a name / WeChat ID to search.
/// Whether an account argument means the logged-in account.
pub fn is_own(api: &Api, arg: &str) -> bool {
  let arg = arg.trim();
  let own = api.ctx.http.cookie("slave_user");
  let appid = api.official().credentials().ok().map(|cr| cr.appid);
  arg.is_empty() || arg == "me" || own.as_deref() == Some(arg) || appid.as_deref() == Some(arg)
}

pub async fn account(api: &Api, arg: &str) -> Result<Account> {
  let arg = arg.trim();
  if is_own(api, arg) {
    return Ok(Account::Own);
  }
  // Paged commands ask again for every page: remember names looked up.
  if let Some(fakeid) = api.accounts.borrow().get(arg) {
    return Ok(Account::Other {
      fakeid: fakeid.clone(),
      user: None,
    });
  }
  let found = resolve(api, arg).await?;
  if let Account::Other { fakeid, .. } = &found {
    api
      .accounts
      .borrow_mut()
      .insert(arg.to_owned(), fakeid.clone());
  }
  Ok(found)
}

async fn resolve(api: &Api, arg: &str) -> Result<Account> {
  if !arg.starts_with("gh_") && refs::bizuin(arg).is_some() {
    return Ok(Account::Other {
      fakeid: arg.to_owned(),
      user: None,
    });
  }
  if arg.contains("mp.weixin.qq.com") {
    if let Ok((biz, _)) = refs::album(arg) {
      return Ok(Account::Other {
        fakeid: biz,
        user: None,
      });
    }
    let post = crate::articles::read(&api.ctx, arg).await?;
    let user = post.author.clone().map(Box::new);
    let fakeid = post
      .extra
      .get("biz")
      .and_then(Value::as_str)
      .ok_or_else(|| Error::upstream("the article page names no account (__biz)"))?;
    return Ok(Account::Other {
      fakeid: fakeid.to_owned(),
      user,
    });
  }
  let found = search(api, arg, 0, 5).await?;
  let pick = found
    .iter()
    .find(|u| {
      u.name == arg
        || u.handle.as_deref() == Some(arg)
        || u.extra.get("username").and_then(Value::as_str) == Some(arg)
    })
    .or_else(|| found.first())
    .cloned()
    .ok_or_else(|| Error::not_found(format!("no official account named {arg}")))?;
  Ok(Account::Other {
    fakeid: pick.id.clone(),
    user: Some(Box::new(pick)),
  })
}

/// Accounts by name or WeChat ID (`searchbiz`, five per page). Accounts can
/// opt out of being found by name.
pub async fn search(api: &Api, query: &str, begin: u64, count: usize) -> Result<Vec<User>> {
  api.backend().require()?;
  let v = api
    .backend()
    .get(
      "/cgi-bin/searchbiz",
      vec![
        ("action", "search_biz".into()),
        ("begin", begin.to_string()),
        ("count", count.to_string()),
        ("query", query.into()),
        ("fingerprint", api.backend().fingerprint()),
      ],
    )
    .await?;
  Ok(v.list("list").iter().map(biz_user).collect())
}

pub async fn search_page(api: &Api, query: &str, req: &PageReq) -> Result<Page<User>> {
  let begin = req.number_or(0);
  let users = search(api, query, begin, 5).await?;
  let next = (users.len() >= 5).then(|| (begin + 5).to_string());
  Ok(Page::new(users, next))
}

pub fn biz_user(b: &Value) -> User {
  let mut user = User {
    id: b.str("fakeid").unwrap_or_default(),
    name: b.str("nickname").unwrap_or_default(),
    handle: b.str("alias"),
    avatar: b.str("round_head_img"),
    bio: b.str("signature"),
    verified: b.i64("verify_status").is_some_and(|s| s > 0),
    raw: Some(b.clone()),
    ..User::default()
  };
  for (key, path) in [("username", "username"), ("service_type", "service_type")] {
    if let Some(v) = b.str(path) {
      user.extra.insert(key.into(), v.into());
    }
  }
  user
}

/// The account behind an argument, as a profile.
pub async fn user(api: &Api, arg: &str) -> Result<User> {
  match account(api, arg).await? {
    Account::Own => crate::users::whoami(api).await,
    Account::Other {
      fakeid,
      user: Some(mut u),
    } => {
      if u.id.starts_with("gh_") {
        u.extra.insert("username".into(), u.id.clone().into());
      }
      u.id = fakeid;
      Ok(*u)
    }
    Account::Other { fakeid, user: None } => Ok(User {
      id: fakeid.clone(),
      name: fakeid,
      ..User::default()
    }),
  }
}
