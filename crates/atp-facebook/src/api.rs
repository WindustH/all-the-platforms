//! What every module works with: the context, the last fetched page (paging
//! replays need its tokens and operation descriptors, so a listing reuses
//! it), the web session, and the Graph API token (also as a Page's own
//! token).

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use atp_core::file::Image;
use atp_core::http::Part;
use atp_core::{Ctx, Error, Result, Value, ValueExt, credentials};
use atp_meta::Graph;
use atp_meta::graph::{self, FACEBOOK, fields};

use crate::refs;
use crate::web::{self, Page};

/// Cookies of a logged-in web session.
pub const SESSION: [&str; 2] = ["c_user", "xs"];
const TOKEN_VARS: [&str; 2] = ["FACEBOOK_ACCESS_TOKEN", "META_ACCESS_TOKEN"];
pub const TOKEN_HINT: &str = "set FACEBOOK_ACCESS_TOKEN (or META_ACCESS_TOKEN), or run \
   `atp facebook login --token <token>`: a user or Page token from the Graph API Explorer \
   or a Meta app (pages_read_engagement, pages_manage_posts, pages_messaging, read_insights, ads_read …)";
/// The Page `use-page` saved.
pub const PAGE_KEY: &str = "page";
const PAGE_FIELDS: &str = "id,name,category,fan_count,followers_count,link,username,tasks";

pub struct Api {
  pub ctx: Ctx,
  last: RefCell<Option<Rc<Page>>>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      last: RefCell::new(None),
    }
  }

  pub fn has_session(&self) -> bool {
    SESSION.iter().all(|c| self.ctx.http.has_cookie(c))
  }

  /// The logged-in account's id (`c_user`).
  pub fn my_id(&self) -> String {
    self.ctx.http.cookie("c_user").unwrap_or_default()
  }

  pub fn require_session(&self, what: &str) -> Result<()> {
    if self.has_session() {
      return Ok(());
    }
    Err(
      Error::auth(format!("{what} needs a logged-in Facebook session"))
        .with_hint("run `atp facebook login --browser` (or `--cookie 'c_user=…; xs=…'`)"),
    )
  }

  pub fn token(&self) -> Option<String> {
    graph::token(&self.ctx, &TOKEN_VARS)
  }

  /// The Graph API with the configured token.
  pub fn graph(&self) -> Result<Graph<'_>> {
    let token = self
      .token()
      .ok_or_else(|| Error::auth("no Graph API access token").with_hint(TOKEN_HINT))?;
    Ok(Graph::new(&self.ctx, FACEBOOK, token))
  }

  /// The Graph API acting as `page` (its own token, looked up with the
  /// user token); the configured token when that lookup fails (it already
  /// is a Page token, or lacks the rights).
  pub async fn page_graph(&self, page: &str) -> Result<Graph<'_>> {
    let graph = self.graph()?;
    match graph.get(page, &fields("access_token")).await {
      Ok(v) => match v.str("access_token") {
        Some(token) => Ok(graph.with_token(token)),
        None => Ok(graph),
      },
      Err(_) => Ok(graph),
    }
  }

  /// The Graph API for a post or comment id: as its Page when the id names
  /// one (`PAGE_POST`), else with the configured token.
  pub async fn graph_for(&self, id: &str) -> Result<Graph<'_>> {
    match refs::owner_of(id) {
      Some(owner) => self.page_graph(owner).await,
      None => self.graph(),
    }
  }

  /// A facebook.com page, reused when the same URL was fetched earlier in
  /// this run.
  pub async fn page(&self, url: &str) -> Result<Rc<Page>> {
    let url = refs::canonical(url);
    if let Some(page) = self.last.borrow().as_ref()
      && page.url == url
    {
      return Ok(page.clone());
    }
    let page = Rc::new(web::fetch(&self.ctx, &url).await?);
    *self.last.borrow_mut() = Some(page.clone());
    Ok(page)
  }

  /// The Pages the token manages; a Page token's `me` is the Page itself.
  pub async fn pages(&self) -> Result<Vec<Value>> {
    let graph = self.graph()?;
    match graph.all("me/accounts", &fields(PAGE_FIELDS), 0).await {
      Ok(items) => Ok(items),
      Err(_) => Ok(vec![graph.get("me", &fields(PAGE_FIELDS)).await?]),
    }
  }

  /// The Page to act on: `given`, FACEBOOK_PAGE_ID, the one `use-page`
  /// saved, or the only one the token manages.
  pub async fn page_id(&self, given: Option<&str>) -> Result<String> {
    if let Some(page) = given
      .map(str::to_owned)
      .or_else(|| credentials::env("FACEBOOK_PAGE_ID"))
      .or_else(|| self.ctx.extra(PAGE_KEY))
    {
      return Ok(page.trim().to_owned());
    }
    match self.pages().await?.as_slice() {
      [one] => Ok(one.str("id").unwrap_or_default()),
      [] => Err(Error::not_found("the token manages no Facebook Page")),
      _ => Err(
        Error::input("which Page? pass it (e.g. --topic PAGE_ID), set FACEBOOK_PAGE_ID or run `atp facebook use-page ID`")
          .with_hint("`atp facebook pages` lists the Pages your token manages"),
      ),
    }
  }
}

/// An image file as the `source` part of a Graph upload.
pub async fn photo_part(path: &Path) -> Result<Part> {
  let image = Image::read(path).await?;
  Ok(Part::file("source", image.data, &image.name, image.mime))
}
