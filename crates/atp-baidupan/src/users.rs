//! The account: who is logged in on either route, and the storage.

use atp_core::{Data, Error, ErrorCode, Result, User, Value, json};

use crate::api::{Api, Route};
use crate::parse;

/// The account behind the web login, else (and right after `login
/// --token`) behind the open-platform token. A web login Baidu no longer
/// accepts falls back to the token when there is one. `logins` says which
/// routes the session holds.
pub async fn whoami(api: &Api) -> Result<User> {
  let open = api.open().logged_in();
  let web = api.web_login();
  if open && (api.token_login.get() || !web) {
    let logins = if web {
      "web, open platform"
    } else {
      "open platform"
    };
    return Ok(with_logins(open_user(api).await?, logins));
  }
  if !web {
    return Err(Error::auth("not logged in").with_hint(api.ctx.login_hint()));
  }
  match web_user(api).await {
    Ok(user) => Ok(with_logins(
      user,
      if open { "web, open platform" } else { "web" },
    )),
    Err(e) if e.code == ErrorCode::NotAuthenticated && open => Ok(with_logins(
      open_user(api).await?,
      "open platform (the web login expired)",
    )),
    Err(e) => Err(e),
  }
}

fn with_logins(mut user: User, logins: &str) -> User {
  user.extra.insert("logins".into(), json!(logins));
  user
}

async fn web_user(api: &Api) -> Result<User> {
  let info = api.login_status().await?;
  let quota = api
    .get("/api/quota", &[("checkfree", "1".into())])
    .await
    .unwrap_or(Value::Null);
  Ok(parse::web_user(&info, &quota))
}

async fn open_user(api: &Api) -> Result<User> {
  let query = [("method", "uinfo".to_owned()), ("vip_version", "v2".into())];
  let v = api.open().get("/nas", &query).await?;
  Ok(parse::open_user(&v))
}

/// Storage used, free and total.
pub async fn quota(api: &Api) -> Result<Data> {
  let query = [("checkfree", "1".to_owned()), ("checkexpire", "1".into())];
  let v = match api.route()? {
    Route::Web => api.get("/api/quota", &query).await?,
    Route::Open => {
      api
        .open()
        .get("https://pan.baidu.com/api/quota", &query)
        .await?
    }
  };
  Ok(Data::Value(parse::quota(&v)))
}
