//! The shared command set and how it is dispatched to a [`Platform`].
//!
//! Every platform gets the same subcommands; the ones it does not support are
//! hidden from `--help`, and platform-specific values (sort orders, hot
//! categories ...) are injected into the shared options from
//! [`Choices`](crate::platform::Choices).

mod account;
mod args;
mod chat;
mod command;
mod input;
mod library;
mod media;
pub mod prompt;
mod read;
mod threads;
mod write;

use std::process::ExitCode;

use clap::{ArgMatches, FromArgMatches, Subcommand, builder::PossibleValuesParser};

pub use self::account::{AccountCommand, LoginArgs};
pub use self::args::{GlobalArgs, PageArgs};
pub use self::chat::ChatCommand;
pub use self::command::CommonCommand;
use self::command::{Outcome, Run};
pub use self::input::{Change, check_files, check_images, confirm, read_text};
pub use self::library::LibraryCommand;
pub use self::media::{DownloadArgs, MediaCommand};
pub use self::read::{ReadCommand, SearchArgs, SearchKind};
pub use self::write::{PublishArgs, WriteCommand};

use crate::account::Loaded;
use crate::ctx::Ctx;
use crate::error::{Error, ErrorCode, Result};
use crate::geo::{self, System};
use crate::http::{Http, HttpConfig};
use crate::model::Data;
use crate::output;
use crate::platform::{Cap, Platform, PlatformInfo};
use crate::store::{RefKind, Store};
use crate::verify;

/// Which capability a shared subcommand needs; `None` means always available.
fn required_cap(name: &str) -> Option<&'static [Cap]> {
  Some(match name {
    "search" => &[Cap::Search, Cap::SearchUsers, Cap::SearchTopics],
    "hot" => &[Cap::Hot],
    "feed" => &[Cap::Feed],
    "read" => &[Cap::Read],
    "comments" => &[Cap::Comments],
    "replies" => &[Cap::Replies],
    "likers" => &[Cap::Likers],
    "reposts" => &[Cap::Reposts],
    "insights" => &[Cap::AccountInsights, Cap::PostInsights],
    "user" => &[Cap::User],
    "user-posts" => &[Cap::UserPosts],
    "followers" => &[Cap::Followers],
    "following" => &[Cap::Following],
    "collections" => &[Cap::Collections],
    "favorites" => &[Cap::Favorites],
    "likes" => &[Cap::Likes],
    "history" => &[Cap::History],
    "notifications" => &[Cap::Notifications],
    "unread" => &[Cap::Unread],
    "like" | "unlike" => &[Cap::Like],
    "favorite" | "unfavorite" => &[Cap::Favorite],
    "comment" => &[Cap::Comment],
    "delete-comment" => &[Cap::DeleteComment],
    "follow" | "unfollow" => &[Cap::Follow],
    "post" => &[Cap::Publish],
    "delete" => &[Cap::Delete],
    "download" => &[Cap::Download],
    "chats" => &[Cap::Chats],
    "messages" => &[Cap::Messages],
    "send" => &[Cap::Send],
    "members" => &[Cap::Members],
    "react" => &[Cap::React],
    "edit" => &[Cap::Edit],
    "listen" => &[Cap::Listen],
    _ => return None,
  })
}

fn with_choices(
  cmd: clap::Command,
  sub: &str,
  arg: &str,
  values: &'static [&'static str],
) -> clap::Command {
  cmd.mut_subcommand(sub, |c| {
    c.mut_arg(arg, |a| {
      if values.is_empty() {
        a.hide(true)
      } else {
        a.value_parser(PossibleValuesParser::new(values.iter().copied()))
      }
    })
  })
}

/// Shorter names of shared commands. A platform whose own commands use one
/// of these names keeps it for its command, so the two never collide.
const ALIASES: &[(&str, &str)] = &[
  ("read", "show"),
  ("user-posts", "posts"),
  ("favorites", "bookmarks"),
  ("post", "publish"),
];

/// The clap command of one platform: shared commands plus its extras.
pub fn command<P: Platform>() -> clap::Command {
  let info = P::INFO;
  let mut cmd = clap::Command::new(info.id);
  cmd = CommonCommand::augment_subcommands(cmd);
  cmd = <P::Extra as Subcommand>::augment_subcommands(cmd);
  // Set after augmenting: derived enums would overwrite `about` with their doc comment.
  cmd = cmd
    .about(info.about)
    .long_about(None)
    .visible_aliases(info.aliases.iter().copied())
    .subcommand_required(true)
    .arg_required_else_help(true)
    .after_help(format!(
      "Guide: `atp guide {id}` (references, login, analytics, limits), `atp guide {id} \
       <command>` for one command, `atp guide` for all topics.",
      id = info.id
    ));
  let own = extra_names::<P>();
  for (sub, alias) in ALIASES {
    if !own.iter().any(|n| n == alias) {
      cmd = cmd.mut_subcommand(sub, |c| c.visible_alias(*alias));
    }
  }
  let names: Vec<String> = cmd
    .get_subcommands()
    .map(|s| s.get_name().to_owned())
    .collect();
  for name in names {
    if let Some(caps) = required_cap(&name)
      && !caps.iter().any(|c| info.supports(*c))
    {
      cmd = cmd.mut_subcommand(&name, |c| c.hide(true));
    }
  }
  if !info.supports(Cap::TokenLogin) {
    cmd = cmd.mut_subcommand("login", |c| c.mut_arg("token", |a| a.hide(true)));
  }
  if !info.supports(Cap::CommentImages) {
    cmd = cmd.mut_subcommand("comment", |c| c.mut_arg("images", |a| a.hide(true)));
  }
  cmd = cmd.mut_subcommand("post", |c| write::post_options(c, &info));
  cmd = cmd.mut_subcommand("edit", |c| write::edit_options(c, &info));
  let ch = info.choices;
  cmd = with_choices(cmd, "search", "sort", ch.search_sort);
  cmd = with_choices(cmd, "search", "filter", ch.search_filter);
  cmd = with_choices(cmd, "hot", "category", ch.hot_category);
  cmd = with_choices(cmd, "feed", "kind", ch.feed_kind);
  cmd = with_choices(cmd, "comments", "sort", ch.comment_sort);
  cmd = with_choices(cmd, "notifications", "kind", ch.notification_kind);
  if info.supports(Cap::Files) {
    cmd = drive_words(cmd, info.id);
  }
  if let Some(system) = P::COORD {
    cmd = cmd.arg(coord_arg(system));
  }
  cmd
}

/// `--coord` of map platforms, on every command.
fn coord_arg(default: System) -> clap::Arg {
  clap::Arg::new(COORD)
    .long(COORD)
    .global(true)
    .value_name("SYSTEM")
    .value_parser(clap::value_parser!(System))
    .default_value(default.name())
    .help("Coordinate system of the points you give and the points printed")
}

/// Id of `--coord`.
pub(crate) const COORD: &str = "coord";

/// On a drive the shared commands handle files, not posts.
fn drive_words(cmd: clap::Command, id: &str) -> clap::Command {
  let download = format!(
    "Examples:\n  atp {id} download /Movies/a.mkv -o ~/Downloads\n  atp {id} download /Movies\n\n\
     A download that stopped continues when run again.\nMore: atp guide download"
  );
  let search = format!("Examples:\n  atp {id} search report\n  atp {id} read '#1'");
  cmd
    .mut_subcommand("read", |c| {
      file_arg(c).about("Show a file or folder (path or id), or a share link")
    })
    .mut_subcommand("delete", |c| {
      file_arg(c).about("Move a file or folder to the recycle bin")
    })
    .mut_subcommand("download", |c| {
      let c = hide_arg(hide_arg(file_arg(c), "audio_only"), "split");
      c.about("Download a file, or a folder with everything in it")
        .after_long_help(download)
    })
    .mut_subcommand("search", |c| {
      hide_arg(c, "kind")
        .about("Search the names of your files and folders")
        .after_long_help(search)
    })
}

/// `POST` shown as `FILE`.
fn file_arg(cmd: clap::Command) -> clap::Command {
  if cmd.get_arguments().any(|a| a.get_id() == "post") {
    cmd.mut_arg("post", |a| a.value_name("FILE"))
  } else {
    cmd
  }
}

fn hide_arg(cmd: clap::Command, arg: &str) -> clap::Command {
  if cmd.get_arguments().any(|a| a.get_id() == arg) {
    cmd.mut_arg(arg, |a| a.hide(true))
  } else {
    cmd
  }
}

/// Names and aliases of the platform-only commands.
fn extra_names<P: Platform>() -> Vec<String> {
  <P::Extra as Subcommand>::augment_subcommands(clap::Command::new(""))
    .get_subcommands()
    .flat_map(|s| {
      let aliases = s.get_all_aliases().map(str::to_owned).collect::<Vec<_>>();
      std::iter::once(s.get_name().to_owned()).chain(aliases)
    })
    .collect()
}

/// Names of the platform-only commands.
pub fn extra_commands<P: Platform>() -> Vec<String> {
  <P::Extra as Subcommand>::augment_subcommands(clap::Command::new(""))
    .get_subcommands()
    .map(|s| s.get_name().to_owned())
    .collect()
}

/// Run one platform command and print the result.
pub async fn run<P: Platform>(global: GlobalArgs, matches: ArgMatches) -> ExitCode {
  let format = global.output_format();
  match execute::<P>(&global, &matches).await {
    Ok((mut data, code)) => {
      if !global.raw {
        data.strip_raw();
      }
      output::emit(format, Some(P::INFO.id), &data);
      code
    }
    Err(e) => {
      output::emit_error(format, Some(P::INFO.id), &e);
      ExitCode::from(e.code.exit_code())
    }
  }
}

/// Run one platform command without printing: the data (without `raw`
/// payloads unless `--raw`) or the error.
pub async fn call<P: Platform>(global: GlobalArgs, matches: ArgMatches) -> Result<Data> {
  let (mut data, _) = execute::<P>(&global, &matches).await?;
  if !global.raw {
    data.strip_raw();
  }
  Ok(data)
}

async fn execute<P: Platform>(
  global: &GlobalArgs,
  matches: &ArgMatches,
) -> Result<(Data, ExitCode)> {
  let info: PlatformInfo = P::INFO;
  let mut loaded = crate::account::load_session(&info, &Store::new(info.id))?;
  let error = match attempt::<P>(global, matches, &loaded, false).await {
    Err(e) if browser_clears::<P>(&e) && verify::wanted(global.no_verify) => e,
    done => return done,
  };
  let session = &loaded.session.cookies;
  let Some(cookies) = verify::complete(&info, &error, session, global.proxy.as_deref()).await
  else {
    return Err(error);
  };
  loaded.session.cookies.extend(cookies);
  attempt::<P>(global, matches, &loaded, true).await
}

/// Whether passing `error` in the person's browser can clear it: a human
/// check whose cookies the platform's requests carry back. Others are
/// printed with their page at once.
fn browser_clears<P: Platform>(error: &Error) -> bool {
  error.code == ErrorCode::VerificationRequired && !error.unclearable && P::BROWSER_CHECKS
}

/// Run the command once on `loaded`; `verified` saves the session even when
/// the run itself changed nothing (the cookies of a passed check).
async fn attempt<P: Platform>(
  global: &GlobalArgs,
  matches: &ArgMatches,
  loaded: &Loaded,
  verified: bool,
) -> Result<(Data, ExitCode)> {
  let info: PlatformInfo = P::INFO;
  let http = Http::new(&http_config(global, &info), loaded.session.cookies.clone())?;
  let store = Store::new(info.id);
  let mut ctx = Ctx::new(info, http, store, loaded.session.extra.clone());
  // Only map platforms define `--coord` (asking clap for it elsewhere panics).
  ctx.coord = P::COORD.and_then(|_| matches.get_one::<System>(COORD).copied());
  let coord = ctx.coord;
  let platform = P::new(ctx)?;

  let name = matches.subcommand_name().unwrap_or_default();
  let mut outcome = if CommonCommand::has_subcommand(name) {
    let cmd = CommonCommand::from_arg_matches(matches).map_err(|e| Error::input(e.to_string()))?;
    let run = Run {
      loaded,
      format: global.output_format(),
      raw: global.raw,
    };
    cmd.run(&platform, &run).await?
  } else {
    let cmd = <P::Extra as FromArgMatches>::from_arg_matches(matches)
      .map_err(|e| Error::input(e.to_string()))?;
    Outcome::saved(platform.run_extra(cmd).await?)
  };

  let ctx = platform.ctx();
  // Only logged-in sessions are saved; guest cookies are not worth keeping.
  let changed = verified || ctx.session_changed();
  if outcome.persist && platform.logged_in() && !loaded.from_env && changed {
    let mut session = ctx.session();
    session.source = loaded.session.source.clone();
    session.saved_at = loaded.session.saved_at;
    ctx.store.save_session(&session)?;
  }
  if let Some(system) = coord {
    geo::localize(&mut outcome.data, system);
  }
  remember(ctx, &outcome.data);
  Ok((outcome.data, outcome.code))
}

fn http_config(global: &GlobalArgs, info: &PlatformInfo) -> HttpConfig {
  HttpConfig {
    cookie_domains: info.cookie_domains,
    proxy: global.proxy.clone(),
    timeout: std::time::Duration::from_secs(global.timeout),
    min_interval: global
      .interval
      .map(|s| std::time::Duration::from_secs_f64(s.max(0.0)))
      .map_or(info.min_interval, |d| d.max(info.min_interval)),
  }
}

/// Remember the references of a printed list, for `#N` in later commands.
fn remember(ctx: &Ctx, data: &Data) {
  match data {
    Data::Posts(page) => ctx.store.remember(
      RefKind::Post,
      page
        .items
        .iter()
        .map(|p| p.reference().to_owned())
        .collect(),
    ),
    Data::Users(page) => ctx.store.remember(
      RefKind::User,
      page
        .items
        .iter()
        .map(|u| u.reference().to_owned())
        .collect(),
    ),
    // A drive's `read`, `download` and `delete` take files where others take posts.
    Data::Files(page) => ctx.store.remember(
      RefKind::Post,
      page
        .items
        .iter()
        .map(|f| f.reference().to_owned())
        .collect(),
    ),
    Data::Collections(page) => ctx.store.remember(
      RefKind::Collection,
      page
        .items
        .iter()
        .map(|c| c.reference().to_owned())
        .collect(),
    ),
    _ => {}
  }
}
