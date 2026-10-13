//! Discord-only commands: server details, roles, emoji, invites, pins and
//! friends.

use social_core::cli::PageArgs;
use social_core::http::Method;
use social_core::{Data, Error, Result, Value, ValueExt, json};

use crate::api::{Api, Body};
use crate::refs::{self, Chat};
use crate::{messages, parse, users};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// About a server: members online, boosts, features, channels and roles
  #[command(visible_alias = "guild")]
  Server { server: String },
  /// Roles of a server
  Roles { server: String },
  /// Custom emoji of a server (the `name:id` form `react` takes)
  Emojis { server: String },
  /// Pinned messages of a channel
  Pins {
    chat: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Pin a message, or unpin it with --undo
  Pin {
    message: String,
    #[arg(long)]
    undo: bool,
  },
  /// What an invite leads to (works without logging in)
  Invite {
    /// Invite code or link (discord.gg/…, discord.com/invite/…)
    code: String,
  },
  /// Open invites of a server (needs the Manage Server permission)
  Invites { server: String },
  /// Friends and friend requests of a user account
  Friends {
    /// Also blocked accounts
    #[arg(long)]
    all: bool,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Server { server } => Data::Value(server_detail(api, &server_id(&server)?).await?),
    Command::Roles { server } => {
      let v = api
        .get(&format!("/guilds/{}/roles", server_id(&server)?), &[])
        .await?;
      let mut roles: Vec<Value> = v
        .as_array()
        .map(|rows| {
          rows
            .iter()
            .map(|r| {
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
            })
            .collect()
        })
        .unwrap_or_default();
      roles.sort_by_key(|r| std::cmp::Reverse(r.i64("position").unwrap_or_default()));
      Data::Value(json!(roles))
    }
    Command::Emojis { server } => {
      let v = api
        .get(&format!("/guilds/{}/emojis", server_id(&server)?), &[])
        .await?;
      let emojis: Vec<Value> = v
        .as_array()
        .map(|rows| {
          rows
            .iter()
            .map(|e| {
              json!({
                "name": e.str("name"),
                "id": e.str("id"),
                "react_with": parse::emoji_name(e),
                "animated": e.bool("animated"),
                "available": e.bool("available"),
              })
            })
            .collect()
        })
        .unwrap_or_default();
      Data::Value(json!(emojis))
    }
    Command::Pins { chat, page } => Data::Posts(
      page
        .collect_dated(async |r| messages::pins(api, &chat, &r).await)
        .await?,
    ),
    Command::Pin { message, undo } => Data::Action(messages::pin(api, &message, undo).await?),
    Command::Invite { code } => Data::Value(invite(api, &code).await?),
    Command::Invites { server } => {
      let v = api
        .get(&format!("/guilds/{}/invites", server_id(&server)?), &[])
        .await?;
      let rows: Vec<Value> = v
        .as_array()
        .map(|rows| rows.iter().map(invite_summary).collect())
        .unwrap_or_default();
      Data::Value(json!(rows))
    }
    Command::Friends { all } => Data::Users(users::relationships(api, all).await?),
  })
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
