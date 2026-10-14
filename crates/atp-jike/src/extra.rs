//! Jike commands beyond the shared set.

use atp_core::cli::PageArgs;
use atp_core::{Data, Page, Result};

use crate::api::Api;
use crate::topics;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// The posts of a topic (圈子), or with `--info` the topic and its tabs
  #[command(after_long_help = EX_TOPIC)]
  Topic {
    /// Topic: link, id, or #N after `collections` or `search --type topic`
    topic: String,
    /// Which of the topic's tabs to list (as `--info` names them); its first by default
    #[arg(long)]
    tab: Option<String>,
    /// Show the topic itself (description, members, tabs) instead of its posts
    #[arg(long)]
    info: bool,
    #[command(flatten)]
    page: PageArgs,
  },
}

const EX_TOPIC: &str = "\
Examples:
  atp jike search 'AI' --type topic
  atp jike topic '#1' -n 30
  atp jike topic https://web.okjike.com/topic/5738965b6628391200809ff1 --info
  atp jike collections                  # the topics you joined

More: atp guide jike topic";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  match command {
    Command::Topic {
      topic,
      tab,
      info,
      page,
    } => {
      let topic = api.ctx.collection_ref(&topic)?;
      if info {
        let found = topics::topic(api, &topic).await?;
        return Ok(Data::Collections(Page::last(vec![found])));
      }
      let posts = page
        .collect_dated(async |req| topics::posts(api, &topic, tab.as_deref(), &req).await)
        .await?;
      Ok(Data::Posts(posts))
    }
  }
}
