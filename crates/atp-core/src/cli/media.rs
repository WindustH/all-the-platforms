//! Downloads of a post's images, video and audio.

use std::path::PathBuf;

use clap::Subcommand;

use crate::download::{self, DownloadOpts};
use crate::error::{Error, Result};
use crate::model::Data;
use crate::platform::{Cap, Platform};

#[derive(Debug, clap::Args)]
pub struct DownloadArgs {
  /// Post: an id, a link, or #N from the last list
  pub post: String,
  /// Directory to save into
  #[arg(short = 'o', long, default_value = ".")]
  pub dir: PathBuf,
  /// Keep only the audio track
  #[arg(long)]
  pub audio_only: bool,
  /// Also split the audio into WAV segments of this many seconds (16 kHz mono, for speech recognition)
  #[arg(long, value_name = "SECS")]
  pub split: Option<u32>,
}

#[derive(Debug, Subcommand)]
pub enum MediaCommand {
  /// Download the images / video / audio of a post
  #[command(after_long_help = EX_DOWNLOAD)]
  Download(DownloadArgs),
}

pub(super) async fn run<P: Platform>(p: &P, cmd: MediaCommand) -> Result<Data> {
  let MediaCommand::Download(args) = cmd;
  if P::INFO.supports(Cap::Files) {
    return files(p, args).await;
  }
  let ctx = p.ctx();
  let audio_only = args.audio_only || args.split.is_some();
  let (post, media) = p.media(&ctx.post_ref(&args.post)?, audio_only).await?;
  let opts = DownloadOpts {
    dir: args.dir,
    audio_only: args.audio_only,
    split: args.split,
  };
  let saved = download::download(
    ctx,
    P::INFO.home,
    &post,
    &media,
    &opts,
    async |m, dir, name| p.save_media(m, dir, name).await,
  )
  .await?;
  Ok(Data::Downloads(saved))
}

/// A drive's file, or a folder with everything in it.
async fn files<P: Platform>(p: &P, args: DownloadArgs) -> Result<Data> {
  if args.audio_only || args.split.is_some() {
    return Err(Error::input(
      "--audio-only and --split apply to media, not to drive files",
    ));
  }
  let ctx = p.ctx();
  let root = p.file(&ctx.post_ref(&args.post)?).await?;
  let saved = download::files(
    ctx,
    &root,
    &args.dir,
    async |folder, req| p.files(folder, &req).await,
    async |file, renew| p.file_link(file, renew).await,
  )
  .await?;
  Ok(Data::Downloads(saved))
}

const EX_DOWNLOAD: &str = "\
Examples:
  atp bili download BV1xx411c7mD -o ~/Videos
  atp bili download BV1xx411c7mD --audio-only --split 25
  atp xhs download '#1'
  atp yt download dQw4w9WgXcQ --audio-only

Needs ffmpeg for merging video and audio, audio extraction and --split.
More: atp guide download";
