//! Favorites folders: creating, changing and deleting them, and moving
//! posts in and out.

use std::path::PathBuf;

use clap::Subcommand;

use super::follows::several;
use super::input::{check_images, confirm};
use crate::error::{Error, Result};
use crate::model::Data;
use crate::platform::{Cap, FolderSpec, Platform};

#[derive(Debug, Subcommand)]
pub enum FolderCommand {
  /// Create a favorites folder
  #[command(after_long_help = EX_FOLDER_CREATE)]
  FolderCreate {
    /// Name of the folder
    name: String,
    /// Description
    #[arg(long, value_name = "TEXT")]
    desc: Option<String>,
    /// Only you can see it
    #[arg(long)]
    private: bool,
  },
  /// Rename a favorites folder, or change its description, visibility or cover
  #[command(after_long_help = EX_FOLDER_EDIT)]
  FolderEdit {
    /// Folder: its id, or #N after `collections`
    folder: String,
    /// New name
    #[arg(long)]
    name: Option<String>,
    /// New description; an empty one clears it
    #[arg(long, value_name = "TEXT")]
    desc: Option<String>,
    /// Make it private
    #[arg(long, conflicts_with = "public")]
    private: bool,
    /// Make it public
    #[arg(long)]
    public: bool,
    /// New cover image
    #[arg(long, value_name = "PATH")]
    cover: Option<PathBuf>,
  },
  /// Delete favorites folders
  FolderDelete {
    /// Folders: ids, or #N after `collections`
    #[arg(required = true, value_name = "FOLDER")]
    folders: Vec<String>,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
  /// Save posts into a favorites folder
  #[command(after_long_help = EX_FOLDER_ADD)]
  FolderAdd {
    /// Folder: its id, or #N after `collections`
    folder: String,
    /// Posts: ids, links, or #N from the last list
    #[arg(required = true, value_name = "POST")]
    posts: Vec<String>,
  },
  /// Take posts out of a favorites folder
  FolderRemove {
    /// Folder: its id, or #N after `collections`
    folder: String,
    /// Posts: ids, links, or #N from the last list
    #[arg(required = true, value_name = "POST")]
    posts: Vec<String>,
  },
  /// Move posts from one favorites folder to another (--copy keeps them in both)
  #[command(after_long_help = EX_FOLDER_MOVE)]
  FolderMove {
    /// Folder the posts are in: its id, or #N after `collections`
    from: String,
    /// Folder to put them in: its id, or #N after `collections`
    to: String,
    /// Posts: ids, links, or #N from the last list
    #[arg(required = true, value_name = "POST")]
    posts: Vec<String>,
    /// Copy them: they stay in FROM too
    #[arg(long)]
    copy: bool,
  },
}

/// Options of `folder-create` / `folder-edit` and the capabilities that
/// show them.
pub(super) const FOLDER_OPTIONS: &[(&str, Cap)] = &[
  ("desc", Cap::FolderDescription),
  ("private", Cap::FolderPrivacy),
  ("public", Cap::FolderPrivacy),
  ("cover", Cap::FolderCover),
];

pub(super) async fn run<P: Platform>(p: &P, cmd: FolderCommand) -> Result<Data> {
  let ctx = p.ctx();
  let posts =
    |list: &[String]| -> Result<Vec<String>> { list.iter().map(|x| ctx.post_ref(x)).collect() };
  Ok(match cmd {
    FolderCommand::FolderCreate {
      name,
      desc,
      private,
    } => {
      let spec = FolderSpec {
        name: Some(name),
        description: desc,
        private: private.then_some(true),
        cover: None,
      };
      Data::Action(
        p.folder_create(&checked(&spec, P::INFO.caps, "folder-create")?)
          .await?,
      )
    }
    FolderCommand::FolderEdit {
      folder,
      name,
      desc,
      private,
      public,
      cover,
    } => {
      let spec = FolderSpec {
        name,
        description: desc,
        private: (private || public).then_some(private),
        cover,
      };
      let spec = checked(&spec, P::INFO.caps, "folder-edit")?;
      Data::Action(p.folder_edit(&ctx.collection_ref(&folder)?, &spec).await?)
    }
    FolderCommand::FolderDelete { folders, yes } => {
      let folders = folders
        .iter()
        .map(|f| ctx.collection_ref(f))
        .collect::<Result<Vec<_>>>()?;
      confirm(yes, &format!("delete {}", several("folder", &folders)))?;
      Data::actions(p.folder_delete(&folders).await?)
    }
    FolderCommand::FolderAdd {
      folder,
      posts: list,
    } => Data::actions(
      p.folder_add(&ctx.collection_ref(&folder)?, &posts(&list)?)
        .await?,
    ),
    FolderCommand::FolderRemove {
      folder,
      posts: list,
    } => Data::actions(
      p.folder_remove(&ctx.collection_ref(&folder)?, &posts(&list)?)
        .await?,
    ),
    FolderCommand::FolderMove {
      from,
      to,
      posts: list,
      copy,
    } => {
      let (from, to) = (ctx.collection_ref(&from)?, ctx.collection_ref(&to)?);
      if from == to {
        return Err(Error::input("FROM and TO are the same folder"));
      }
      Data::actions(p.folder_move(&from, &to, &posts(&list)?, copy).await?)
    }
  })
}

/// The folder as given, after refusing what the platform does not offer
/// and changes that change nothing.
fn checked(spec: &FolderSpec, caps: &[Cap], command: &str) -> Result<FolderSpec> {
  let given = [
    (spec.description.is_some(), Cap::FolderDescription, "--desc"),
    (
      spec.private.is_some(),
      Cap::FolderPrivacy,
      "--private / --public",
    ),
    (spec.cover.is_some(), Cap::FolderCover, "--cover"),
  ];
  if let Some((_, _, option)) = given.iter().find(|(on, cap, _)| *on && !caps.contains(cap)) {
    return Err(Error::unsupported(format!("{command} {option}")));
  }
  if spec.name.as_deref().is_some_and(|n| n.trim().is_empty()) {
    return Err(Error::input("the folder name is empty"));
  }
  if spec.name.is_none() && !given.iter().any(|(on, _, _)| *on) {
    return Err(Error::input(
      "nothing to change: give --name, --desc, --private, --public or --cover",
    ));
  }
  if let Some(cover) = &spec.cover {
    check_images(std::slice::from_ref(cover))?;
  }
  Ok(FolderSpec {
    name: spec.name.as_deref().map(|n| n.trim().to_owned()),
    ..spec.clone()
  })
}

const EX_FOLDER_CREATE: &str = "\
Examples:
  atp bili folder-create 'Watch again'
  atp bili folder-create Drafts --desc 'for later' --private

--desc and --private exist where the platform has them (folder-description,
folder-privacy in `atp platforms`). More: atp guide <platform> folder-create";

const EX_FOLDER_EDIT: &str = "\
Examples:
  atp bili collections
  atp bili folder-edit '#2' --name 'Music videos' --public
  atp bili folder-edit 1052622027 --desc '' --cover cover.jpg

More: atp guide <platform> folder-edit";

const EX_FOLDER_ADD: &str = "\
Examples:
  atp bili collections
  atp bili search 'rust 教程' -n 10
  atp bili folder-add '#2' '#1' '#3' '#4'
  atp bili folder-remove 1052622027 BV1xx411c7mD

FOLDER comes from the last collection list, POST from the last post list.
More: atp guide <platform> folder-add";

const EX_FOLDER_MOVE: &str = "\
Examples:
  atp bili collections
  atp bili favorites --folder '#1' -n 50
  atp bili folder-move '#1' '#3' '#2' '#5' '#8'
  atp bili folder-move 1052622027 1052622127 BV1xx411c7mD --copy

More: atp guide <platform> folder-move";
