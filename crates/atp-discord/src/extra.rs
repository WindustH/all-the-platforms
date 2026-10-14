//! Discord-only commands: server details, roles, emoji, invites, pins and
//! friends.

use atp_core::cli::PageArgs;
use atp_core::http::Method;
use atp_core::{Data, Error, Result, Value, ValueExt, json};

use crate::api::{Api, Body};
use crate::refs::{self, Chat};
use crate::{parse, pins, users};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// About a server: members online, boosts, features, channels and roles
  #[command(visible_alias = "guild", after_long_help = EX_SERVER)]
  Server {
    /// Server: id, `server:ID`, a discord.com/channels/SERVER link, or #N after `chats`
    server: String,
  },
  /// Roles of a server
  Roles {
    /// Server: id, `server:ID`, a discord.com/channels/SERVER link, or #N after `chats`
    server: String,
  },
  /// Custom emoji of a server (the `name:id` form `react` takes)
  #[command(after_long_help = EX_EMOJIS)]
  Emojis {
    /// Server: id, `server:ID`, a discord.com/channels/SERVER link, or #N after `chats`
    server: String,
  },
  /// Pinned messages of a channel
  Pins {
    /// Channel: id, link, `<#ID>`, `channel:ID`, a user for your direct chat, or #N after `chats`
    chat: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Pin a message, or unpin it with --undo
  Pin {
    /// Message: link, `CHANNEL/MESSAGE`, `CHANNEL-MESSAGE`, or #N from the last list
    message: String,
    /// Unpin it instead
    #[arg(long)]
    undo: bool,
  },
  /// What an invite leads to (works without logging in)
  #[command(after_long_help = EX_INVITE)]
  Invite {
    /// Invite code or link (discord.gg/…, discord.com/invite/…)
    code: String,
  },
  /// Open invites of a server (needs the Manage Server permission)
  Invites {
    /// Server: id, `server:ID`, a discord.com/channels/SERVER link, or #N after `chats`
    server: String,
  },
  /// Friends and friend requests of a user account
  Friends {
    /// Also blocked accounts
    #[arg(long)]
    all: bool,
  },
}

const EX_SERVER: &str = "\
Examples:
  atp discord server 81384788765712384
  atp discord server https://discord.com/channels/81384788765712384

More: atp guide discord server";

const EX_EMOJIS: &str = "\
Examples:
  atp discord emojis 81384788765712384
  atp discord react 81384788862181376/1234567890123456789 party:81384788865712384

More: atp guide discord emojis";

const EX_INVITE: &str = "\
Examples:
  atp discord invite rust-lang
  atp discord invite https://discord.gg/rust-lang

More: atp guide discord invite";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let server_ref = |arg: &str| server_id(&api.ctx.collection_ref(arg)?);
  Ok(match command {
    Command::Server { server } => Data::Value(server_detail(api, &server_ref(&server)?).await?),
    Command::Roles { server } => Data::Value(roles(api, &server_ref(&server)?).await?),
    Command::Emojis { server } => Data::Value(emojis(api, &server_ref(&server)?).await?),
    Command::Pins { chat, page } => {
      let chat = api.ctx.collection_ref(&chat)?;
      let pinned = page.collect_dated(async |r| pins::list(api, &chat, &r).await);
      Data::Posts(pinned.await?)
    }
    Command::Pin { message, undo } => {
      Data::Action(pins::pin(api, &api.ctx.post_ref(&message)?, undo).await?)
    }
    Command::Invite { code } => Data::Value(invite(api, &code).await?),
    Command::Invites { server } => Data::Value(invites(api, &server_ref(&server)?).await?),
    Command::Friends { all } => Data::Users(users::relationships(api, all).await?),
  })
}

/// Each element of an array answer, mapped.
fn each(v: &Value, f: impl Fn(&Value) -> Value) -> Vec<Value> {
  v.as_array()
    .map(|rows| rows.iter().map(f).collect())
    .unwrap_or_default()
}

/// Roles, highest first.
async fn roles(api: &Api, server: &str) -> Result<Value> {
  let v = api.get(&format!("/guilds/{server}/roles"), &[]).await?;
  let mut roles = each(&v, |r| {
    json!({
      "id": r.str("id"),
      "name": r.str("name"),
      "position": r.i64("position"),
      "color": r.i64("color").map(|c| format!("#{c:06x}")),
      "hoist": r.bool("hoist"),
      "mentionable": r.bool("mentionable"),
      "managed": r.bool("managed"),
      "permissions": r.str("permissions"),
    })
  });
  roles.sort_by_key(|r| std::cmp::Reverse(r.i64("position").unwrap_or_default()));
  Ok(json!(roles))
}

async fn emojis(api: &Api, server: &str) -> Result<Value> {
  let v = api.get(&format!("/guilds/{server}/emojis"), &[]).await?;
  Ok(json!(each(&v, |e| {
    json!({
      "name": e.str("name"),
      "id": e.str("id"),
      "react_with": parse::emoji_name(e),
      "animated": e.bool("animated"),
      "available": e.bool("available"),
    })
  })))
}

async fn invites(api: &Api, server: &str) -> Result<Value> {
  let v = api.get(&format!("/guilds/{server}/invites"), &[]).await?;
  Ok(json!(each(&v, invite_summary)))
}

fn server_id(arg: &str) -> Result<String> {
  match refs::chat(arg)? {
    Chat::Server(id) | Chat::Id(id) => Ok(id),
    _ => Err(Error::input(format!("not a server: {arg}"))),
  }
}

async fn server_detail(api: &Api, id: &str) -> Result<Value> {
  let v = api
    .get(&format!("/guilds/{id}"), &[("with_counts", "true".into())])
    .await?;
  let mut c = parse::guild(&v);
  c.raw = None;
  let extra = &mut c.extra;
  for (key, path) in [
    ("owner_id", "owner_id"),
    ("vanity_code", "vanity_url_code"),
    ("locale", "preferred_locale"),
    ("rules_channel", "rules_channel_id"),
    ("system_channel", "system_channel_id"),
  ] {
    if let Some(s) = v.str(path) {
      extra.insert(key.into(), json!(s));
    }
  }
  for (key, path) in [
    ("boost_tier", "premium_tier"),
    ("boosts", "premium_subscription_count"),
    ("verification_level", "verification_level"),
    ("max_members", "max_members"),
  ] {
    if let Some(n) = v.u64(path) {
      extra.insert(key.into(), json!(n));
    }
  }
  extra.insert("roles".into(), json!(v.list("roles").len()));
  extra.insert("emojis".into(), json!(v.list("emojis").len()));
  extra.insert("stickers".into(), json!(v.list("stickers").len()));
  let features = v.list("features");
  if !features.is_empty() {
    extra.insert("features".into(), json!(features));
  }
  Ok(json!(c))
}

/// An invite, looked up anonymously (with the session's token if any).
async fn invite(api: &Api, arg: &str) -> Result<Value> {
  let code = arg
    .trim()
    .trim_end_matches('/')
    .rsplit('/')
    .next()
    .unwrap_or_default()
    .to_owned();
  if code.is_empty() {
    return Err(Error::input("give an invite code or link"));
  }
  let token = api.token();
  let v = api
    .send(
      token.as_ref(),
      Method::GET,
      &format!("/invites/{code}"),
      &[
        ("with_counts", "true".into()),
        ("with_expiration", "true".into()),
      ],
      Body::None,
    )
    .await?;
  let mut out = invite_summary(&v);
  if let Some(o) = out.as_object_mut() {
    if let Some(n) = v.u64("approximate_member_count") {
      o.insert("members".into(), json!(n));
    }
    if let Some(n) = v.u64("approximate_presence_count") {
      o.insert("online".into(), json!(n));
    }
    if let Some(d) = v.str("guild.description") {
      o.insert("description".into(), json!(d));
    }
  }
  Ok(out)
}

fn invite_summary(v: &Value) -> Value {
  json!({
    "code": v.str("code"),
    "url": v.str("code").map(|c| format!("https://discord.gg/{c}")),
    "server": v.str("guild.name"),
    "server_id": v.str("guild.id"),
    "channel": v.str("channel.name"),
    "channel_id": v.str("channel.id"),
    "inviter": v.first_str(&["inviter.global_name", "inviter.username"]),
    "uses": v.u64("uses"),
    "max_uses": v.u64("max_uses"),
    "expires_at": v.str("expires_at"),
  })
}
