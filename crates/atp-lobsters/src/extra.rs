//! Lobsters commands beyond the shared set.

use atp_core::cli::PageArgs;
use atp_core::{Data, Error, Result};

use crate::api::Api;
use crate::stories;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Stories with a tag, as on its page
  Tag {
    /// Tag name (`rust`), or a /t/ link
    tag: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Every tag, with its description and category
  Tags {
    #[command(flatten)]
    page: PageArgs,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Tag { tag, page } => {
      let tag = tag_name(&api.ctx.collection_ref(&tag)?)?;
      let base = format!("t/{tag}");
      Data::Posts(
        page
          .collect_dated(async |req| stories::listing(api, &base, &req).await)
          .await?,
      )
    }
    Command::Tags { page } => Data::Collections(
      page
        .collect(async |req| stories::tags(api, &req).await)
        .await?,
    ),
  })
}

/// `rust`, `/t/rust` or `https://lobste.rs/t/rust` → `rust`.
fn tag_name(arg: &str) -> Result<String> {
  let name = arg
    .trim()
    .trim_end_matches('/')
    .rsplit("/t/")
    .next()
    .unwrap_or_default()
    .trim_start_matches(['/', '#']);
  let valid = !name.is_empty()
    && name
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b"_-.+".contains(&b));
  if valid {
    Ok(name.to_owned())
  } else {
    Err(Error::input(format!("not a Lobsters tag: {arg}")))
  }
}
