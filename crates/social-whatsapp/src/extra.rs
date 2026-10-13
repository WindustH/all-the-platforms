//! WhatsApp-only commands.

use social_core::{Action, Data, Result};

use crate::api::{Api, Mode, not_logged_in};
use crate::{chats, groups, login, messages, users};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Link this computer with a code typed on the phone instead of a QR code
  LoginCode {
    /// The account's phone number with its country code
    phone: String,
  },
  /// Remove the linked device on WhatsApp and delete its keys and archive
  Unlink,
  /// Ask the phone for older messages of a chat (the phone must be online)
  Older {
    chat: String,
    /// How many messages to ask for
    #[arg(short = 'n', long, default_value_t = 50)]
    count: i32,
  },
  /// Mark a chat (personal) or a message (Cloud API) as read
  MarkRead { target: String },
  /// Your groups, from the server
  Groups,
  /// Which phone numbers are on WhatsApp
  Check {
    #[arg(required = true)]
    numbers: Vec<String>,
  },
  /// Cloud API: the business profile
  Profile,
  /// Cloud API: the business account's phone numbers
  Numbers,
  /// Cloud API: message templates
  Templates,
  /// Cloud API: send an approved template (opens a conversation)
  Template {
    to: String,
    name: String,
    /// Template language code
    #[arg(long, default_value = "en_US")]
    lang: String,
    /// A body parameter, in order (repeatable)
    #[arg(short = 'p', long = "param")]
    params: Vec<String>,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::LoginCode { phone } => Data::User(Box::new(login::pair_code(api, &phone).await?)),
    Command::Unlink => Data::Action(unlink(api).await?),
    Command::Older { chat, count } => {
      let chat = ctx.collection_ref(&chat)?;
      Data::Action(chats::older(api, &chat, count).await?)
    }
    Command::MarkRead { target } => Data::Action(messages::mark_read(api, &target).await?),
    Command::Groups => Data::Collections(
      api
        .on_device("groups", async |link| groups::groups(link).await)
        .await?,
    ),
    Command::Check { numbers } => Data::Users(
      api
        .on_device("check", async |link| users::check(link, &numbers).await)
        .await?,
    ),
    Command::Profile => Data::Value(api.cloud("profile")?.profile(ctx).await?),
    Command::Numbers => Data::Users(api.cloud("numbers")?.numbers(ctx).await?),
    Command::Templates => Data::Collections(api.cloud("templates")?.templates(ctx).await?),
    Command::Template {
      to,
      name,
      lang,
      params,
    } => Data::Action(
      api
        .cloud("template")?
        .send_template(ctx, &to, &name, &lang, &params)
        .await?,
    ),
  })
}

/// Unlink the device, or forget Cloud API credentials.
async fn unlink(api: &Api) -> Result<Action> {
  match api.mode() {
    Mode::Personal => api.personal.unlink(&api.ctx).await,
    Mode::Cloud(_) => {
      api.forget_cloud();
      Ok(Action::done("unlink", "cloud-api").with_message("credentials forgotten"))
    }
    Mode::None => Err(not_logged_in()),
  }
}
