//! Stack Exchange commands beyond the shared set.

use atp_core::cli::{PageArgs, read_text};
use atp_core::{Data, Error, Result, ValueExt, json};

use crate::api::{Api, ENV_SITE, SITE};
use crate::{parse, posts, search, write};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Answer a question (Markdown; needs an access token)
  #[command(after_long_help = EX_ANSWER)]
  Answer {
    /// Question: link, id or #N
    question: String,
    /// The answer in Markdown; `-` reads it from stdin
    text: Option<String>,
  },
  /// Vote a question or answer down, or take the vote back with --undo
  Downvote {
    /// Question or answer: link, id or #N
    post: String,
    /// Take your downvote back
    #[arg(long)]
    undo: bool,
  },
  /// Tags of the site, most used first
  Tags {
    /// Only tags whose names contain this
    words: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Questions with a tag
  #[command(after_long_help = EX_TAG)]
  Tag {
    /// Tag name; several joined with `;` must all be present
    tag: String,
    /// Order
    #[arg(short, long, default_value = "active",
      value_parser = ["active", "newest", "votes", "hot", "week", "month"])]
    sort: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Show the default site, or set it
  #[command(after_long_help = EX_SITE)]
  Site {
    /// Site name (`superuser`) or domain (`math.stackexchange.com`)
    name: Option<String>,
  },
  /// The sites of the Stack Exchange network
  Sites {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Requests left today for this IP or app key
  Quota,
}

const EX_ANSWER: &str = "\
Examples:
  atp so answer 11227809 'Branch prediction: …'
  atp so answer '#1' - < answer.md

More: atp guide so answer";

const EX_TAG: &str = "\
Examples:
  atp so tag rust -n 20
  atp so tag 'rust;async' --sort votes

More: atp guide so tag";

const EX_SITE: &str = "\
Examples:
  atp so site                  # the default site
  atp so site superuser        # make Super User the default
  SE_SITE=math atp so hot      # one run on Mathematics

More: atp guide so site";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Answer { question, text } => {
      let body = read_text(text)?;
      Data::Action(write::answer(api, &ctx.post_ref(&question)?, body.trim()).await?)
    }
    Command::Downvote { post, undo } => {
      Data::Action(write::vote(api, &ctx.post_ref(&post)?, false, undo).await?)
    }
    Command::Tags { words, page } => Data::Collections(
      page
        .collect(async |req| search::tags(api, words.as_deref(), &req).await)
        .await?,
    ),
    Command::Tag { tag, sort, page } => Data::Posts(
      page
        .collect_dated(async |req| posts::tagged(api, &tag, &sort, &req).await)
        .await?,
    ),
    Command::Site { name } => site(api, name)?,
    Command::Sites { page } => Data::Collections(
      page
        .collect(async |req| {
          api
            .list("sites", None, Vec::new(), "default", &req, parse::site)
            .await
        })
        .await?,
    ),
    Command::Quota => quota(api).await?,
  })
}

/// The default site, after saving `name` as the new one.
fn site(api: &Api, name: Option<String>) -> Result<Data> {
  if let Some(name) = name {
    let name = name
      .trim()
      .trim_start_matches("https://")
      .trim_end_matches('/');
    if name.is_empty() || name.contains(char::is_whitespace) {
      return Err(Error::input("expected a site name such as `superuser`"));
    }
    api.ctx.store.update_session(|session| {
      session.extra.insert(SITE.to_owned(), name.to_owned());
    })?;
    api.ctx.set_extra(SITE, name);
  }
  let from = if atp_core::credentials::env(ENV_SITE).is_some() {
    ENV_SITE
  } else if api.ctx.extra(SITE).is_some() {
    "saved"
  } else {
    "default"
  };
  Ok(Data::Value(json!({"site": api.site(), "from": from})))
}

async fn quota(api: &Api) -> Result<Data> {
  let site = api.site();
  let v = api.get("info", Some(&site), Vec::new(), "default").await?;
  Ok(Data::Value(json!({
    "left": v.u64("quota_remaining"),
    "max": v.u64("quota_max"),
    "key": api.key().is_some(),
  })))
}
