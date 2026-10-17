//! Multireddits: reading them, and managing your own.

use atp_core::cli::{PageArgs, confirm};
use atp_core::{Data, Result};
use clap::builder::PossibleValuesParser;

use super::SORTS;
use crate::api::Api;
use crate::multis::{self, MultiSpec, VISIBILITY};
use crate::posts::TIME_RANGES;

#[derive(Debug, clap::Subcommand)]
pub enum MultiCommand {
  /// Your multireddits, or a user's public ones
  #[command(visible_alias = "multireddits")]
  Multis {
    /// User: name, `u/name` or profile link
    user: Option<String>,
  },
  /// Posts of a multireddit (`u/USER/m/NAME`, a link or `#N` after `multis`)
  #[command(after_long_help = EX_MULTI)]
  Multi {
    /// Multireddit: `u/USER/m/NAME`, `USER/NAME`, link or #N
    multireddit: String,
    /// Order of the posts
    #[arg(short, long, default_value = "hot", value_parser = SORTS)]
    sort: String,
    /// Time range of `top` and `controversial`
    #[arg(short, long, value_parser = PossibleValuesParser::new(TIME_RANGES.iter().copied()))]
    time: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Create a multireddit (custom feed), optionally with subreddits
  #[command(after_long_help = EX_MULTI_CREATE)]
  MultiCreate {
    /// Its name (up to 50 characters; the link takes its ASCII letters and digits)
    name: String,
    /// Subreddits to put in: names, `r/name`, links, or #N after a subreddit list
    #[arg(value_name = "SUBREDDIT")]
    subreddits: Vec<String>,
    /// Description (Markdown)
    #[arg(long, value_name = "TEXT")]
    desc: Option<String>,
    /// Who sees it
    #[arg(long, default_value = "private", value_parser = PossibleValuesParser::new(VISIBILITY))]
    visibility: String,
  },
  /// Rename one of your multireddits, or change its description or visibility
  MultiUpdate {
    /// Your multireddit: `u/USER/m/NAME`, link or #N after `multis`
    multireddit: String,
    /// New name (the link stays)
    #[arg(long)]
    name: Option<String>,
    /// New description (Markdown); an empty one clears it
    #[arg(long, value_name = "TEXT")]
    desc: Option<String>,
    /// Who sees it
    #[arg(long, value_parser = PossibleValuesParser::new(VISIBILITY))]
    visibility: Option<String>,
  },
  /// Delete one of your multireddits (its subreddits stay subscribed)
  MultiDelete {
    /// Your multireddit: `u/USER/m/NAME`, link or #N after `multis`
    multireddit: String,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
  /// Add subreddits to one of your multireddits
  #[command(after_long_help = EX_MULTI_ADD)]
  MultiAdd {
    /// Your multireddit: `u/USER/m/NAME`, link or #N after `multis`
    multireddit: String,
    /// Subreddits: names, `r/name`, links, or #N after a subreddit list
    #[arg(required = true, value_name = "SUBREDDIT")]
    subreddits: Vec<String>,
  },
  /// Take subreddits out of one of your multireddits
  MultiRemove {
    /// Your multireddit: `u/USER/m/NAME`, link or #N after `multis`
    multireddit: String,
    /// Subreddits: names, `r/name`, links, or #N after a subreddit list
    #[arg(required = true, value_name = "SUBREDDIT")]
    subreddits: Vec<String>,
  },
}

const EX_MULTI: &str = "\
Examples:
  atp reddit multis spez
  atp reddit multi '#1'
  atp reddit multi u/spez/m/tech --sort top --time month

More: atp guide reddit multi";

const EX_MULTI_CREATE: &str = "\
Examples:
  atp reddit multi-create 'Rust news' rust r/programming --desc 'daily reading'
  atp reddit multi-create Space nasa spacex --visibility public

More: atp guide reddit multi-create";

const EX_MULTI_ADD: &str = "\
Examples:
  atp reddit multi-add u/me/m/rust_news r/learnrust r/rust_gamedev
  atp reddit multi-remove u/me/m/rust_news learnrust

More: atp guide reddit multi-add";

pub(super) async fn run(api: &Api, command: MultiCommand) -> Result<Data> {
  let ctx = &api.ctx;
  let subs = |list: &[String]| -> Result<Vec<String>> {
    list.iter().map(|s| ctx.collection_ref(s)).collect()
  };
  Ok(match command {
    MultiCommand::Multis { user } => {
      let user = user.map(|u| ctx.user_ref(&u)).transpose()?;
      Data::Collections(multis::list(api, user.as_deref()).await?)
    }
    MultiCommand::Multi {
      multireddit,
      sort,
      time,
      page,
    } => {
      let multi = ctx.collection_ref(&multireddit)?;
      Data::Posts(
        page
          .collect_dated(async |r| multis::posts(api, &multi, &sort, time.as_deref(), &r).await)
          .await?,
      )
    }
    MultiCommand::MultiCreate {
      name,
      subreddits,
      desc,
      visibility,
    } => {
      let spec = MultiSpec {
        name: Some(&name),
        description: desc.as_deref(),
        visibility: Some(&visibility),
      };
      Data::Action(multis::create(api, &spec, &subs(&subreddits)?).await?)
    }
    MultiCommand::MultiUpdate {
      multireddit,
      name,
      desc,
      visibility,
    } => update(api, &multireddit, name, desc, visibility).await?,
    MultiCommand::MultiDelete { multireddit, yes } => {
      let multi = ctx.collection_ref(&multireddit)?;
      confirm(yes, &format!("delete multireddit {multi}"))?;
      Data::Action(multis::delete(api, &multi).await?)
    }
    MultiCommand::MultiAdd {
      multireddit,
      subreddits,
    } => {
      let multi = ctx.collection_ref(&multireddit)?;
      Data::actions(multis::file(api, &multi, &subs(&subreddits)?, false).await?)
    }
    MultiCommand::MultiRemove {
      multireddit,
      subreddits,
    } => {
      let multi = ctx.collection_ref(&multireddit)?;
      Data::actions(multis::file(api, &multi, &subs(&subreddits)?, true).await?)
    }
  })
}

async fn update(
  api: &Api,
  multireddit: &str,
  name: Option<String>,
  desc: Option<String>,
  visibility: Option<String>,
) -> Result<Data> {
  let spec = MultiSpec {
    name: name.as_deref(),
    description: desc.as_deref(),
    visibility: visibility.as_deref(),
  };
  let multi = api.ctx.collection_ref(multireddit)?;
  Ok(Data::Action(multis::update(api, &multi, &spec).await?))
}
