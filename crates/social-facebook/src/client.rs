//! What the modules share: the context, the last fetched page (paging replays
//! need its tokens and operation descriptors, so a listing reuses it), and
//! the Graph API token.

use std::cell::RefCell;
use std::rc::Rc;

use social_core::{Ctx, Error, Result};
use social_meta::Graph;
use social_meta::graph::{self, FACEBOOK};

use crate::web::{self, Page};

/// Cookies of a logged-in web session.
pub const SESSION: [&str; 2] = ["c_user", "xs"];
const TOKEN_VARS: [&str; 2] = ["FACEBOOK_ACCESS_TOKEN", "META_ACCESS_TOKEN"];
pub const TOKEN_HINT: &str = "set FACEBOOK_ACCESS_TOKEN (or META_ACCESS_TOKEN), or run \
   `social facebook login --token <token>`: a user or Page token from the Graph API Explorer \
   or a Meta app (pages_read_engagement, pages_manage_posts, pages_messaging, read_insights, ads_read …)";

pub struct Client {
  pub ctx: Ctx,
  last: RefCell<Option<Rc<Page>>>,
}

impl Client {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      last: RefCell::new(None),
    }
  }

  pub fn has_session(&self) -> bool {
    SESSION.iter().all(|c| self.ctx.http.has_cookie(c))
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

  /// A page, reused when the same URL was fetched earlier in this run.
  pub async fn page(&self, url: &str) -> Result<Rc<Page>> {
    let url = web::canonical(url);
    if let Some(p) = self.last.borrow().as_ref()
      && p.url == url
    {
      return Ok(p.clone());
    }
    let page = Rc::new(web::page(&self.ctx, &url).await?);
    *self.last.borrow_mut() = Some(page.clone());
    Ok(page)
  }
}
