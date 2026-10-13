//! A subreddit's rules, moderators, wiki and flairs, and a post's flair.

use social_core::{Data, Result};

use crate::api::Api;
use crate::{moderation, subreddits};

#[derive(Debug, clap::Subcommand)]
pub enum CommunityCommand {
  /// A subreddit's rules
  Rules { subreddit: String },
  /// A subreddit's moderators and their permissions
  #[command(visible_alias = "moderators")]
  Mods { subreddit: String },
  /// A subreddit's wiki: its pages, or one page (`index` is the main one)
  Wiki {
    subreddit: String,
    page: Option<String>,
  },
  /// Post flairs a subreddit offers
  Flairs { subreddit: String },
  /// Set your post's flair: a template from `flairs` and/or a text
  Flair {
    post: String,
    /// Flair text (where the subreddit or template allows editing)
    text: Option<String>,
    /// Template id, or `#N` after `flairs`
    #[arg(long)]
    template: Option<String>,
  },
}

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
