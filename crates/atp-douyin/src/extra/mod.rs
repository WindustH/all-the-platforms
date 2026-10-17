//! Douyin-only commands; those on your library are in [`library`].

mod library;

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::{login, posts, rooms};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Log in with a phone number and the code Douyin texts to it
  #[command(after_long_help = EX_LOGIN_SMS)]
  LoginSms {
    /// Mainland number, or `+CC NUMBER` (asked when left out)
    phone: Option<String>,
  },
  /// Posts of a mix (合集)
  Mix {
    /// Mix id or douyin.com/collection/… link
    mix: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Posts that use a music
  Music {
    /// Music id or douyin.com/music/… link
    music: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Posts Douyin recommends next to one
  Related {
    /// Post: aweme id, douyin.com link, v.douyin.com short link, shared text or #N
    post: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// A live room: title, anchor, audience and stream addresses
  Live {
    /// web_rid or live.douyin.com/… link
    room: String,
  },
  /// Your collections (合集), which `post --collection` takes
  Mixes {
    #[command(flatten)]
    page: PageArgs,
  },
  /// The music library of image posts and articles, for `post --music`
  #[command(after_long_help = EX_MUSICS)]
  Musics {
    /// A category id from the list without it (TYPE:ID); shows its songs
    category: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  #[command(flatten)]
  Library(library::LibraryCommand),
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::LoginSms { phone } => Data::User(Box::new(login::sms(api, phone).await?)),
    Command::Mix { mix, page } => Data::Posts(
      page
        .collect(async |r| crate::library::mix(api, &mix, &r).await)
        .await?,
    ),
    Command::Music { music, page } => Data::Posts(
      page
        .collect_dated(async |r| crate::library::music(api, &music, &r).await)
        .await?,
    ),
    Command::Related { post, page } => {
      let post = api.ctx.post_ref(&post)?;
      Data::Posts(
        page
          .collect(async |r| posts::related(api, &post, &r).await)
          .await?,
      )
    }
    Command::Live { room } => Data::Post(Box::new(rooms::room(api, &room).await?)),
    Command::Mixes { page } => Data::Collections(
      page
        .collect(async |r| crate::library::mixes(api, &r).await)
        .await?,
    ),
    Command::Musics { category, page } => Data::Collections(
      page
        .collect(async |r| crate::library::musics(api, category.as_deref(), &r).await)
        .await?,
    ),
    Command::Library(c) => library::run(api, c).await?,
  })
}

const EX_LOGIN_SMS: &str = "\
Examples:
  atp douyin login-sms                # asks for the number, then the code
  atp douyin login-sms 13800138000

The code is typed in on the terminal. A captcha stops the login with
verification_required.
More: atp guide douyin login-sms";

const EX_MUSICS: &str = "\
Examples:
  atp douyin musics                   # the categories
  atp douyin musics 1:7 -n 40         # the songs of one
  atp douyin post -i a.jpg -i b.jpg --title 标题 --music 7123456789012345678

More: atp guide douyin musics";
