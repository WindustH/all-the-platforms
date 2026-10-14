//! Smaller interactions: comment likes and pins, video danmaku, pinned dynamics.

use atp_core::{Data, Error, Result};

use super::video_ref;
use crate::api::Api;
use crate::refs::{self, PostRef};
use crate::{comments, dynamics, media};

#[derive(Debug, clap::Subcommand)]
pub enum InteractCommand {
  /// Like a comment (--dislike to mark it down instead; --undo removes either)
  #[command(after_long_help = EX_COMMENT_LIKE)]
  CommentLike {
    /// Post the comment is under: video, dynamic or article (id, link or #N)
    post: String,
    /// Comment id (`ROOT:PARENT` for a reply in a thread)
    comment: String,
    /// Mark the comment down (踩) instead of liking it
    #[arg(long)]
    dislike: bool,
    /// Remove the like or the dislike
    #[arg(long)]
    undo: bool,
  },
  /// Pin a comment under your own post (--undo unpins)
  CommentPin {
    /// Your post: video, dynamic or article (id, link or #N)
    post: String,
    /// Comment id
    comment: String,
    /// Unpin the comment
    #[arg(long)]
    undo: bool,
  },
  /// Send danmaku to a video
  #[command(after_long_help = EX_DANMAKU_SEND)]
  DanmakuSend {
    /// Video: BV… or av… id, link (`?p=N` for a part), b23.tv short link or #N
    video: String,
    /// The danmaku text
    text: String,
    /// Seconds into the video (or the part, with ?p=N)
    #[arg(long, default_value_t = 0.0)]
    at: f64,
    /// Color as hex, e.g. ff0000
    #[arg(long, default_value = "ffffff")]
    color: String,
    /// How it moves: across the screen, or fixed at the top or bottom
    #[arg(long, default_value = "scroll", value_parser = ["scroll", "bottom", "top"])]
    mode: String,
  },
  /// Pin one of your dynamics on your space (--undo unpins)
  Pin {
    /// Your dynamic: id, t.bilibili.com or opus link, or #N
    dynamic: String,
    /// Unpin it
    #[arg(long)]
    undo: bool,
  },
}

const EX_COMMENT_LIKE: &str = "\
Examples:
  atp bili comment-like BV1xx411c7mD 237849322112
  atp bili comment-like BV1xx411c7mD 237849322112:237850911040 --dislike
  atp bili comment-like BV1xx411c7mD 237849322112 --undo

More: atp guide bili comment-like";

const EX_DANMAKU_SEND: &str = "\
Examples:
  atp bili danmaku-send BV1xx411c7mD 'hello' --at 42.5
  atp bili danmaku-send BV1xx411c7mD '前方高能' --at 90 --color ff0000 --mode top";

pub async fn run(api: &Api, command: InteractCommand) -> Result<Data> {
  use InteractCommand as I;
  Ok(match command {
    I::CommentLike {
      post,
      comment,
      dislike,
      undo,
    } => {
      let section = section(api, &post).await?;
      let (_, rpid) = refs::reply_target(&comment)?;
      Data::Action(comments::react(api, &section, &rpid, dislike, undo).await?)
    }
    I::CommentPin {
      post,
      comment,
      undo,
    } => {
      let section = section(api, &post).await?;
      let (_, rpid) = refs::reply_target(&comment)?;
      Data::Action(comments::pin(api, &section, &rpid, undo).await?)
    }
    I::DanmakuSend {
      video,
      text,
      at,
      color,
      mode,
    } => {
      let v = video_ref(api, &video).await?;
      let color = u32::from_str_radix(color.trim_start_matches('#'), 16)
        .map_err(|_| Error::input(format!("bad color: {color} (hex, e.g. ff0000)")))?;
      let mode = match mode.as_str() {
        "bottom" => 4,
        "top" => 5,
        _ => 1,
      };
      Data::Action(media::send_danmaku(api, &v, &text, at, color, mode).await?)
    }
    I::Pin { dynamic, undo } => match refs::post(api, &api.ctx.post_ref(&dynamic)?).await? {
      PostRef::Dynamic(id) => Data::Action(dynamics::pin(api, &id, undo).await?),
      _ => return Err(Error::input("pin takes a dynamic")),
    },
  })
}

async fn section(api: &Api, post: &str) -> Result<comments::Section> {
  let post = refs::post(api, &api.ctx.post_ref(post)?).await?;
  comments::section(api, &post).await
}
