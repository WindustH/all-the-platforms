//! Smaller interactions: comment likes and pins, video danmaku, pinned dynamics.

use social_core::{Ctx, Data, Error, Result};

use super::video_ref;
use crate::refs::{self, PostRef};
use crate::{comment, dynamic, play};

#[derive(Debug, clap::Subcommand)]
pub enum Interact {
  /// Like a comment (--dislike to mark it down instead; --undo removes either)
  CommentLike {
    post: String,
    comment: String,
    #[arg(long)]
    dislike: bool,
    #[arg(long)]
    undo: bool,
  },
  /// Pin a comment under your own post (--undo unpins)
  CommentPin {
    post: String,
    comment: String,
    #[arg(long)]
    undo: bool,
  },
  /// Send danmaku to a video
  DanmakuSend {
    video: String,
    text: String,
    /// Seconds into the video (or the part, with ?p=N)
    #[arg(long, default_value_t = 0.0)]
    at: f64,
    /// Color as hex, e.g. ff0000
    #[arg(long, default_value = "ffffff")]
    color: String,
    #[arg(long, default_value = "scroll", value_parser = ["scroll", "bottom", "top"])]
    mode: String,
  },
  /// Pin one of your dynamics on your space (--undo unpins)
  Pin {
    dynamic: String,
    #[arg(long)]
    undo: bool,
  },
}

pub async fn run(ctx: &Ctx, command: Interact) -> Result<Data> {
  use Interact as I;
  Ok(match command {
    I::CommentLike {
      post,
      comment,
      dislike,
      undo,
    } => {
      let section = section(ctx, &post).await?;
      let (_, rpid) = refs::reply_target(&comment)?;
      Data::Action(comment::react(ctx, &section, &rpid, dislike, undo).await?)
    }
    I::CommentPin {
      post,
      comment,
      undo,
    } => {
      let section = section(ctx, &post).await?;
      let (_, rpid) = refs::reply_target(&comment)?;
      Data::Action(comment::pin(ctx, &section, &rpid, undo).await?)
    }
    I::DanmakuSend {
      video,
      text,
      at,
      color,
      mode,
    } => {
      let v = video_ref(ctx, &video).await?;
      let color = u32::from_str_radix(color.trim_start_matches('#'), 16)
        .map_err(|_| Error::input(format!("bad color: {color} (hex, e.g. ff0000)")))?;
      let mode = match mode.as_str() {
        "bottom" => 4,
        "top" => 5,
        _ => 1,
      };
      Data::Action(play::send_danmaku(ctx, &v, &text, at, color, mode).await?)
    }
    I::Pin { dynamic, undo } => match refs::post(ctx, &ctx.post_ref(&dynamic)?).await? {
      PostRef::Dynamic(id) => Data::Action(dynamic::pin(ctx, &id, undo).await?),
      _ => return Err(Error::input("pin takes a dynamic")),
    },
  })
}

async fn section(ctx: &Ctx, post: &str) -> Result<comment::Section> {
  let post = refs::post(ctx, &ctx.post_ref(post)?).await?;
  comment::section(ctx, &post).await
}
