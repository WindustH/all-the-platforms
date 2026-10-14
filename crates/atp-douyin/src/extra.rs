//! Douyin-only commands.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::{library, login, posts, rooms};

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
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::LoginSms { phone } => Data::User(Box::new(login::sms(api, phone).await?)),
    Command::Mix { mix, page } => Data::Posts(
      page
        .collect(async |r| library::mix(api, &mix, &r).await)
        .await?,
    ),
    Command::Music { music, page } => Data::Posts(
      page
        .collect_dated(async |r| library::music(api, &music, &r).await)
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
  })
}

const EX_LOGIN_SMS: &str = "\
Examples:
  atp douyin login-sms                # asks for the number, then the code
  atp douyin login-sms 13800138000

The code is typed in on the terminal. A captcha stops the login with
verification_required.
More: atp guide douyin login-sms";
