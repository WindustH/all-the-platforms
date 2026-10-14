//! A subreddit's rules, moderators, wiki and flairs, and a post's flair.

use atp_core::{Data, Result};

use crate::api::Api;
use crate::{moderation, subreddits};

#[derive(Debug, clap::Subcommand)]
pub enum CommunityCommand {
  /// A subreddit's rules
  Rules {
    /// Subreddit: name, r/name or link
    subreddit: String,
  },
  /// A subreddit's moderators and their permissions
  #[command(visible_alias = "moderators")]
  Mods {
    /// Subreddit: name, r/name or link
    subreddit: String,
  },
  /// A subreddit's wiki: its pages, or one page (`index` is the main one)
  #[command(after_long_help = EX_WIKI)]
  Wiki {
    /// Subreddit: name, r/name or link
    subreddit: String,
    /// Page name (`index` is the main one) or #N after the list
    page: Option<String>,
  },
  /// Post flairs a subreddit offers
  Flairs {
    /// Subreddit: name, r/name or link
    subreddit: String,
  },
  /// Set your post's flair: a template from `flairs` and/or a text
  #[command(after_long_help = EX_FLAIR)]
  Flair {
    /// Post: id, `t3_…`, link or #N
    post: String,
    /// Flair text (where the subreddit or template allows editing)
    text: Option<String>,
    /// Template id, or `#N` after `flairs`
    #[arg(long)]
    template: Option<String>,
  },
}

const EX_WIKI: &str = "\
Examples:
  atp reddit wiki rust
  atp reddit wiki r/rust '#2'
  atp reddit wiki AskHistorians faq

More: atp guide reddit wiki";

const EX_FLAIR: &str = "\
Examples:
  atp reddit flairs r/test
  atp reddit flair 1wt65ox --template '#3'
  atp reddit flair 1wt65ox 'Solved' --template '#1'

More: atp guide reddit flair";

pub(super) async fn run(api: &Api, command: CommunityCommand) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    CommunityCommand::Rules { subreddit } => Data::Value(subreddits::rules(api, &subreddit).await?),
    CommunityCommand::Mods { subreddit } => {
      Data::Users(subreddits::moderators(api, &subreddit).await?)
    }
    CommunityCommand::Wiki { subreddit, page } => match page {
      None => Data::Collections(subreddits::wiki_pages(api, &subreddit).await?),
      Some(page) => {
        // `#N` after the page list names `SUB/PAGE`; page names have slashes too.
        let resolved = ctx.collection_ref(&page)?;
        let page = match resolved.split_once('/') {
          Some((_, p)) if resolved != page => p.to_owned(),
          _ => resolved,
        };
        Data::Post(Box::new(
          subreddits::wiki_page(api, &subreddit, &page).await?,
        ))
      }
    },
    CommunityCommand::Flairs { subreddit } => {
      Data::Collections(subreddits::link_flairs(api, &subreddit).await?)
    }
    CommunityCommand::Flair {
      post,
      text,
      template,
    } => {
      let template = template.map(|t| ctx.collection_ref(&t)).transpose()?;
      Data::Action(
        moderation::flair(
          api,
          &ctx.post_ref(&post)?,
          template.as_deref(),
          text.as_deref(),
        )
        .await?,
      )
    }
  })
}
