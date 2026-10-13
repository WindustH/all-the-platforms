//! WhatsApp-only commands.

use social_core::{Action, Data, Error, Result};

use crate::{Mode, WhatsApp, personal};

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

pub async fn run(w: &WhatsApp, command: Command) -> Result<Data> {
  let ctx = &w.ctx;
  let p = &w.personal;
  Ok(match command {
    Command::LoginCode { phone } => {
      let user = p.pair_code(ctx, &phone).await?;
      for key in [crate::cloud::TOKEN, crate::cloud::PHONE, crate::cloud::WABA] {
        ctx.set_extra(key, "");
      }
      Data::User(Box::new(user))
    }
    Command::Unlink => match w.mode() {
      Mode::Personal => Data::Action(p.unlink(ctx).await?),
      Mode::Cloud(_) => {
        for key in [crate::cloud::TOKEN, crate::cloud::PHONE, crate::cloud::WABA] {
          ctx.set_extra(key, "");
        }
        Data::Action(Action::done("unlink", "cloud-api").with_message("credentials forgotten"))
      }
      Mode::None => return Err(w.not_logged_in()),
    },
    Command::Older { chat, count } => {
      let chat = ctx.collection_ref(&chat)?;
      let added = w
        .personal("older", async |l| {
          let me = l.me();
          let jid = crate::refs::chat(&chat, &l.archive, &me)?;
          l.fetch_older(&jid, count.clamp(1, 500)).await
        })
        .await?;
      Data::Action(
        Action::done("older", chat).with_message(format!("{added} older messages archived")),
      )
    }
    Command::MarkRead { target } => match w.mode() {
      Mode::Cloud(c) => Data::Action(c.mark_read(ctx, &ctx.post_ref(&target)?).await?),
      _ => {
        let chat = ctx.collection_ref(&target)?;
        Data::Action(
          w.personal("mark-read", async |l| personal::mark_read(l, &chat).await)
            .await?,
        )
      }
    },
    Command::Groups => Data::Collections(
      w.personal("groups", async |l| personal::groups(l).await)
        .await?,
    ),
    Command::Check { numbers } => Data::Users(
      w.personal("check", async |l| personal::check(l, &numbers).await)
        .await?,
    ),
    Command::Profile => Data::Value(cloud(w)?.profile(ctx).await?),
    Command::Numbers => Data::Users(cloud(w)?.numbers(ctx).await?),
    Command::Templates => Data::Collections(cloud(w)?.templates(ctx).await?),
    Command::Template {
      to,
      name,
      lang,
      params,
    } => Data::Action(
      cloud(w)?
        .send_template(ctx, &to, &name, &lang, &params)
        .await?,
    ),
  })
}

fn cloud(w: &WhatsApp) -> Result<crate::cloud::Cloud> {
  match w.mode() {
    Mode::Cloud(c) => Ok(c),
    Mode::Personal => Err(w.personal_cannot("this command")),
    Mode::None => Err(Error::auth("no Cloud API credentials").with_hint(
      "`social whatsapp login --token TOKEN:PHONE_NUMBER_ID[:WABA_ID]` or WHATSAPP_TOKEN / WHATSAPP_PHONE_NUMBER_ID",
    )),
  }
}
