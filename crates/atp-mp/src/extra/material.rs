//! The material library (素材).

use std::path::PathBuf;

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use super::confirm;
use crate::api::Api;
use crate::material;

#[derive(Debug, clap::Subcommand)]
pub enum MaterialCommand {
  /// The material library (素材)
  Material {
    /// image, video, voice or news
    #[arg(short = 't', long = "type", default_value = "image",
      value_parser = clap::builder::PossibleValuesParser::new(material::KINDS))]
    kind: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// How many materials of each type (API)
  MaterialCount,
  /// Add a permanent material (API)
  #[command(after_long_help = EX_MATERIAL_UPLOAD)]
  MaterialUpload {
    /// The file to upload
    path: PathBuf,
    /// image, thumb, voice or video
    #[arg(short = 't', long = "type", default_value = "image",
      value_parser = clap::builder::PossibleValuesParser::new(["image", "thumb", "voice", "video"]))]
    kind: String,
    /// Video title (default: the file name)
    #[arg(long)]
    title: Option<String>,
    /// Video description
    #[arg(long)]
    intro: Option<String>,
  },
  /// Delete a permanent material (API)
  MaterialDelete {
    /// The material's `media_id`, from `material`
    id: String,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
}

const EX_MATERIAL_UPLOAD: &str = "\
Examples:
  atp mp material-upload banner.png
  atp mp material-upload intro.mp4 -t video --title '开箱' --intro '第一期'
  atp mp material -t video

More: atp guide mp material-upload";

pub async fn run(api: &Api, command: MaterialCommand) -> Result<Data> {
  Ok(match command {
    MaterialCommand::Material { kind, page } => Data::Posts(
      page
        .collect_dated(async |r| material::list(api, &kind, &r).await)
        .await?,
    ),
    MaterialCommand::MaterialCount => Data::Value(material::count(api).await?),
    MaterialCommand::MaterialUpload {
      path,
      kind,
      title,
      intro,
    } => Data::Action(material::add(api, &path, &kind, title.as_deref(), intro.as_deref()).await?),
    MaterialCommand::MaterialDelete { id, yes } => {
      confirm(yes, &format!("delete material {id}"))?;
      Data::Action(material::delete(api, &id).await?)
    }
  })
}
