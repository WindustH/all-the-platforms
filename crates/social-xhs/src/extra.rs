//! Xiaohongshu-only commands.

use std::path::PathBuf;

use social_core::cli::{PageArgs, check_images, read_text};
use social_core::{Action, Data, Draft, Error, Result};

use crate::api::Api;
use crate::{events, feeds, history, insights, interactions, notes, notifications, search, users};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Your own published notes (creator center)
  MyNotes {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Your notes with impressions, views, CTR, watch time and follows (data center)
  NoteStats {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Activities of the creator center (活动中心), optionally matching a keyword
  #[command(after_long_help = EX_EVENTS)]
  Events {
    /// Only activities whose name, reward or topic contains this
    keyword: Option<String>,
    /// Only the activities you kept (收藏)
    #[arg(long)]
    kept: bool,
    /// Latest first instead of the center's own order
    #[arg(long)]
    latest: bool,
  },
  /// One activity: rewards, period, topics and links
  Event {
    /// Activity id, page id, link or exact name
    event: String,
  },
  /// Keep (收藏) an activity in the activity center
  KeepEvent {
    /// Activity id, page id, link or exact name
    event: String,
    /// Remove it from your kept activities instead
    #[arg(long)]
    undo: bool,
  },
  /// Join an activity by publishing an image note for it
  #[command(after_long_help = EX_JOIN)]
  JoinEvent {
    /// Activity id, page id, link or exact name
    event: String,
    /// Note text; `-` reads it from stdin
    text: Option<String>,
    #[arg(long)]
    title: Option<String>,
    /// Attach an image (repeatable, at least one)
    #[arg(short = 'i', long = "image", value_name = "PATH", required = true)]
    images: Vec<PathBuf>,
    /// Another topic besides the activity's own (repeatable)
    #[arg(long = "topic", value_name = "TOPIC")]
    topics: Vec<String>,
  },
  /// The home page's channels (`feed --type` / `hot --category` values)
  Channels,
  /// What the search box suggests before you type (猜你想搜), personalized
  Trending,
  /// Search completions for a keyword
  Suggest { keyword: String },
  /// Search your own collected notes (or with --likes, liked notes)
  SearchSaved {
    keyword: String,
    /// Search the liked notes instead
    #[arg(long)]
    likes: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Like a comment of a note (or take the like back with --undo)
  LikeComment {
    note: String,
    comment: String,
    #[arg(long)]
    undo: bool,
  },
  /// Mark notifications as read (all tabs, or one with --type)
  MarkRead {
    /// mentions, likes or connections
    #[arg(short = 't', long = "type")]
    kind: Option<String>,
  },
  /// How many notes the browsing history holds
  HistoryCount,
  /// Remove notes from the browsing history
  HistoryDelete {
    #[arg(required = true)]
    notes: Vec<String>,
  },
  /// Empty the browsing history of notes
  HistoryClear {
    #[arg(short, long)]
    yes: bool,
  },
  /// Your fans who interacted most (data center)
  ActiveFans {
    /// Window in days: up to 7 means the last 7, otherwise the last 30
    #[arg(short, long, default_value_t = 30)]
    days: u32,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::MyNotes { page } => Data::Posts(
      page
        .collect_dated(async |r| notes::my_notes(api, &r).await)
        .await?,
    ),
    Command::NoteStats { page } => Data::Posts(
      page
        .collect_dated(async |r| insights::note_stats(api, &r).await)
        .await?,
    ),
    Command::Events {
      keyword,
      kept,
      latest,
    } => {
      let rows = events::list(api, kept, latest).await?;
      Data::Collections(events::page(&events::matching(rows, keyword.as_deref())))
    }
    Command::Event { event } => Data::Value(events::detail(&events::find(api, &event).await?)),
    Command::KeepEvent { event, undo } => Data::Action(events::keep(api, &event, undo).await?),
    Command::JoinEvent {
      event,
      text,
      title,
      images,
      topics,
    } => Data::Action(join_event(api, &event, text, title, images, topics).await?),
    Command::Channels => Data::Collections(feeds::channels(api).await?),
    Command::Trending => Data::Collections(search::trending(api).await?),
    Command::Suggest { keyword } => Data::Collections(search::suggest(api, &keyword).await?),
    Command::SearchSaved {
      keyword,
      likes,
      page,
    } => Data::Posts(
      page
        .collect_dated(async |r| users::search_saved(api, &keyword, likes, &r).await)
        .await?,
    ),
    Command::LikeComment {
      note,
      comment,
      undo,
    } => {
      let note = api.ctx.post_ref(&note)?;
      Data::Action(interactions::like_comment(api, &note, &comment, undo).await?)
    }
    Command::MarkRead { kind } => {
      Data::Action(notifications::mark_read(api, kind.as_deref()).await?)
    }
    Command::HistoryCount => Data::Value(history::count(api).await?),
    Command::HistoryDelete { notes } => Data::Action(history_delete(api, &notes).await?),
    Command::HistoryClear { yes } => Data::Action(history_clear(api, yes).await?),
    Command::ActiveFans { days } => Data::Users(insights::active_fans(api, days).await?),
  })
}

async fn join_event(
  api: &Api,
  event: &str,
  text: Option<String>,
  title: Option<String>,
  images: Vec<PathBuf>,
  topics: Vec<String>,
) -> Result<Action> {
  check_images(&images)?;
  let draft = Draft {
    title,
    text: read_text(text)?,
    images,
    topics,
    ..Draft::default()
  };
  events::join(api, event, &draft).await
}

async fn history_delete(api: &Api, notes: &[String]) -> Result<Action> {
  let notes = notes
    .iter()
    .map(|n| api.ctx.post_ref(n))
    .collect::<Result<Vec<_>>>()?;
  history::delete(api, &notes).await
}

async fn history_clear(api: &Api, yes: bool) -> Result<Action> {
  if !yes {
    return Err(Error::input(
      "refusing to clear the browsing history without --yes",
    ));
  }
  history::clear(api).await
}

const EX_EVENTS: &str = "\
Examples:
  social xhs events                 # everything running now
  social xhs events 动漫 --latest    # activities about anime, newest first
  social xhs events --kept          # the ones you kept";

const EX_JOIN: &str = "\
Examples:
  social xhs events 动漫
  social xhs event '#1'             # rewards, period and topics first
  social xhs join-event '#1' --title '十月新番' -i cover.jpg '这季最期待的三部'
  social xhs join-event 43010 --title 周末 -i a.jpg -i b.jpg - < note.txt";
