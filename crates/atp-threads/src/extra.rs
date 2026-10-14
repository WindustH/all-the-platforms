//! Threads commands beyond the shared set.

use atp_core::cli::read_text;
use atp_core::{Action, Data, Error, Result};

use crate::api::Api;
use crate::containers::{self, Compose};
use crate::login;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Publish a post with images, videos or a carousel given by public URL
  #[command(after_long_help = EX_THREAD)]
  Thread {
    /// Text (500 characters at most); `-` reads it from stdin
    text: Option<String>,
    /// Public URL of a JPEG or PNG image (repeatable; two or more make a carousel)
    #[arg(long = "image", value_name = "URL")]
    images: Vec<String>,
    /// Public URL of an MP4 or MOV video (repeatable)
    #[arg(long = "video", value_name = "URL")]
    videos: Vec<String>,
    /// Reply to this post (id, link or #N)
    #[arg(long, value_name = "POST")]
    reply_to: Option<String>,
    /// Quote this post (id, link or #N)
    #[arg(long, value_name = "POST")]
    quote: Option<String>,
    /// A topic tag, without the `#`
    #[arg(long)]
    topic: Option<String>,
    /// A link shown as a preview card (text posts only)
    #[arg(long, value_name = "URL")]
    link: Option<String>,
    /// Who may reply
    #[arg(long, value_parser = ["everyone", "accounts_you_follow", "mentioned_only", "parent_post_author_only", "followers_only"])]
    reply_control: Option<String>,
  },
  /// Repost a post
  Repost {
    /// Post: id, link or #N
    post: String,
  },
  /// Hide a reply to one of your posts, or show it again with --undo
  Hide {
    /// The reply: id, link or #N
    reply: String,
    /// Show it again
    #[arg(long)]
    undo: bool,
  },
  /// How much of today's publishing quota is used
  Quota,
  /// Renew the saved access token now (it is renewed weekly on its own)
  Refresh,
}

const EX_THREAD: &str = "\
Examples:
  atp threads thread 'Morning light' --image https://example.com/photo.jpg
  atp threads thread 'Three views' --image https://example.com/a.jpg --image https://example.com/b.jpg --video https://example.com/c.mp4
  atp threads thread 'Worth a read' --link https://example.com/article --topic books
  atp threads thread 'Agreed' --reply-to '#2' --reply-control accounts_you_follow

More: atp guide threads thread";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Thread {
      text,
      images,
      videos,
      reply_to,
      quote,
      topic,
      link,
      reply_control,
    } => {
      let compose = Compose {
        text: read_text(text)?,
        images,
        videos,
        reply_to,
        quote,
        topic: topic.map(|t| t.trim_start_matches('#').to_owned()),
        link,
        reply_control,
      };
      Data::Action(thread(api, compose).await?)
    }
    Command::Repost { post } => {
      Data::Action(containers::repost(api, &api.ctx.post_ref(&post)?).await?)
    }
    Command::Hide { reply, undo } => {
      Data::Action(containers::hide(api, &api.ctx.post_ref(&reply)?, undo).await?)
    }
    Command::Quota => containers::quota(api).await?,
    Command::Refresh => refresh(api).await?,
  })
}

/// `thread`: `#N` references resolved, then [`containers::create_post`].
async fn thread(api: &Api, mut compose: Compose) -> Result<Action> {
  if compose.text.trim().is_empty() && compose.images.is_empty() && compose.videos.is_empty() {
    return Err(Error::input(
      "nothing to post: give a text, --image or --video",
    ));
  }
  let ctx = &api.ctx;
  compose.reply_to = compose.reply_to.map(|r| ctx.post_ref(&r)).transpose()?;
  compose.quote = compose.quote.map(|q| ctx.post_ref(&q)).transpose()?;
  containers::create_post(api, &compose).await
}

async fn refresh(api: &Api) -> Result<Data> {
  let token = api.token().ok_or_else(|| {
    Error::auth("no Threads access token").with_hint("run `atp threads login --token ACCESS_TOKEN`")
  })?;
  if atp_core::credentials::env(crate::api::ENV_TOKEN).is_some() {
    return Err(Error::input(
      "the token comes from THREADS_ACCESS_TOKEN; renew it where you keep it",
    ));
  }
  let renewed = login::refresh(api, &token).await?;
  let message = format!(
    "renewed for 60 days: {}",
    atp_core::credentials::mask(&renewed)
  );
  Ok(Data::Action(
    Action::done("refresh", "token").with_message(message),
  ))
}
