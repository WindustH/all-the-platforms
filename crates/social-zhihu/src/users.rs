//! Users: profiles, their posts and follow lists, who liked a post, and
//! following or blocking them.

use social_core::{Action, Error, Page, PageReq, Post, Result, User, ValueExt};

use crate::api::{Api, V4};
use crate::refs::Target;
use crate::{api, parse};

pub async fn whoami(api: &Api) -> Result<User> {
  let v = api.call(api.get(&format!("{V4}/me")).query(
      "include",
      "answer_count,articles_count,pins_count,question_count,zvideo_count,columns_count,follower_count,following_count,voteup_count,thanked_count,favorited_count,description",
    ),
  )
  .await?;
  match v.str("id") {
    Some(id) if id != "0" => Ok(parse::user(&v)),
    _ => Err(Error::auth("Zhihu did not accept the session")),
  }
}

/// The logged-in user's `url_token`.
pub async fn my_token(api: &Api) -> Result<String> {
  whoami(api)
    .await?
    .handle
    .ok_or_else(|| Error::upstream("the account has no url_token"))
}

const MEMBER_INCLUDE: &str = "answer_count,articles_count,pins_count,question_count,zvideo_count,columns_count,follower_count,following_count,following_question_count,following_topic_count,following_columns_count,following_favlists_count,voteup_count,thanked_count,favorited_count,included_answers_count,included_articles_count,is_following,is_followed,gender,badge,description,business,educations,employments,locations";

const LIST_INCLUDE: &str =
  "data[*].answer_count,articles_count,follower_count,gender,is_followed,is_following,badge";

pub async fn user(api: &Api, token: &str) -> Result<User> {
  let v = api
    .call(
      api
        .get(&format!("{V4}/members/{token}"))
        .query("include", MEMBER_INCLUDE),
    )
    .await?;
  Ok(parse::user(&v))
}

/// One page of an offset-paged v4 listing.
pub async fn answers(api: &Api, token: &str, page: &PageReq) -> Result<Page<Post>> {
  let include = "data[*].content,excerpt,voteup_count,comment_count,favlists_count,thanks_count,visit_count,reaction,created_time,updated_time,question";
  let path = format!("members/{token}/answers?sort_by=created");
  Ok(api::posts(
    &api.list(&path, Some(include), page).await?,
    parse::answer,
  ))
}

pub async fn articles(api: &Api, token: &str, page: &PageReq) -> Result<Page<Post>> {
  let include = "data[*].content,excerpt,voteup_count,comment_count,favlists_count,reaction,created,updated,image_url,topics";
  let path = format!("members/{token}/articles?sort_by=created");
  Ok(api::posts(
    &api.list(&path, Some(include), page).await?,
    parse::article,
  ))
}

pub async fn pins(api: &Api, token: &str, page: &PageReq) -> Result<Page<Post>> {
  let path = format!("members/{token}/pins");
  Ok(api::posts(&api.list(&path, None, page).await?, parse::pin))
}

/// Followers (`followers`) or followees (`followees`) of a user.
pub async fn follows(api: &Api, token: &str, which: &str, page: &PageReq) -> Result<Page<User>> {
  let v = api
    .list(
      &format!("members/{token}/{which}"),
      Some(LIST_INCLUDE),
      page,
    )
    .await?;
  Ok(Page::new(
    v.list("data").iter().map(parse::user).collect(),
    api::next_param(&v, "offset"),
  ))
}

/// Users who upvoted (赞同) an answer, article or pin, newest first
/// (`GET_UPVOTERS` of the web app, static.zhihu.com/heifetz/main.app.*.js).
pub async fn likers(api: &Api, target: &Target, page: &PageReq) -> Result<Page<User>> {
  if let Target::Question(_) = target {
    return Err(Error::input(
      "Zhihu lists upvoters of answers, articles and pins, not of questions",
    ));
  }
  let path = format!("{}/{}/upvoters", target.plural(), target.id());
  let v = api.list(&path, None, page).await?;
  Ok(Page::new(
    v.list("data").iter().map(parse::user).collect(),
    api::next_param(&v, "offset"),
  ))
}

pub(crate) async fn owner(api: &Api, user: Option<&str>) -> Result<String> {
  match user {
    Some(u) => Ok(u.to_owned()),
    None => my_token(api).await,
  }
}

/// Follow a user (`members`) or a question (`questions`).
pub async fn follow(api: &Api, resource: &str, id: &str, undo: bool) -> Result<Action> {
  api.prepare_write().await?;
  let url = format!("{V4}/{resource}/{id}/followers");
  let req = if undo {
    api.delete(&url)
  } else {
    api.post(&url)
  };
  api.call(req).await?;
  Ok(Action::done(if undo { "unfollow" } else { "follow" }, id))
}

/// Block a user (屏蔽 / 拉黑), or unblock.
pub async fn block(api: &Api, token: &str, undo: bool) -> Result<Action> {
  api.prepare_write().await?;
  let url = format!("{V4}/members/{token}/actions/block");
  let req = if undo {
    api.delete(&url)
  } else {
    api.post(&url)
  };
  api.call(req).await?;
  Ok(Action::done(if undo { "unblock" } else { "block" }, token))
}
