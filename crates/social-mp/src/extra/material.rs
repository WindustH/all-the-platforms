//! The material library (素材).

use std::path::PathBuf;

use social_core::cli::PageArgs;
use social_core::{Data, Result};

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
  MaterialUpload {
    path: PathBuf,
    /// image, thumb, voice or video
    #[arg(short = 't', long = "type", default_value = "image",
      value_parser = clap::builder::PossibleValuesParser::new(["image", "thumb", "voice", "video"]))]
    kind: String,
    /// Video title
    #[arg(long)]
    title: Option<String>,
    /// Video description
    #[arg(long)]
    intro: Option<String>,
  },
  /// Delete a permanent material (API)
  MaterialDelete {
    id: String,
    #[arg(short, long)]
    yes: bool,
  },
}

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
