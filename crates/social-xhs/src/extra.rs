//! Xiaohongshu-only commands.

use std::path::PathBuf;

use social_core::cli::{PageArgs, check_images, read_text};
use social_core::{Data, Draft, Result};

use crate::api::Client;
use crate::{creator, discover, events, history, inbox, people, write};

#[derive(Debug, clap::Subcommand)]
pub enum XhsCommand {
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

pub async fn run(c: &Client, command: XhsCommand) -> Result<Data> {
  Ok(match command {
    XhsCommand::MyNotes { page } => Data::Posts(
      page
        .collect_dated(async |r| creator::my_notes(c, &r).await)
        .await?,
    ),
    XhsCommand::NoteStats { page } => Data::Posts(
      page
        .collect_dated(async |r| creator::note_stats(c, &r).await)
        .await?,
    ),
    XhsCommand::Events {
      keyword,
      kept,
      latest,
    } => {
      let rows = events::list(c, kept, latest).await?;
      Data::Collections(events::page(&events::matching(rows, keyword.as_deref())))
    }
    XhsCommand::Event { event } => Data::Value(events::detail(&events::find(c, &event).await?)),
    XhsCommand::KeepEvent { event, undo } => Data::Action(events::keep(c, &event, undo).await?),
    XhsCommand::JoinEvent {
      event,
      text,
      title,
      images,
      topics,
    } => {
      check_images(&images)?;
      let draft = Draft {
        title,
        text: read_text(text)?,
        images,
        topics,
        ..Draft::default()
      };
      Data::Action(events::join(c, &event, &draft).await?)
    }
    XhsCommand::Channels => Data::Collections(discover::channels(c).await?),
    XhsCommand::Trending => Data::Collections(discover::trending(c).await?),
    XhsCommand::Suggest { keyword } => Data::Collections(discover::suggest(c, &keyword).await?),
    XhsCommand::SearchSaved {
      keyword,
      likes,
      page,
    } => Data::Posts(
      page
        .collect_dated(async |r| people::search_saved(c, &keyword, likes, &r).await)
        .await?,
    ),
    XhsCommand::LikeComment {
      note,
      comment,
      undo,
    } => {
      let note = c.ctx.post_ref(&note)?;
      Data::Action(write::like_comment(c, &note, &comment, undo).await?)
    }
    XhsCommand::MarkRead { kind } => Data::Action(inbox::mark_read(c, kind.as_deref()).await?),
    XhsCommand::HistoryCount => Data::Value(history::count(c).await?),
    XhsCommand::HistoryDelete { notes } => {
      let notes = notes
        .iter()
        .map(|n| c.ctx.post_ref(n))
        .collect::<Result<Vec<_>>>()?;
      Data::Action(history::delete(c, &notes).await?)
    }
    XhsCommand::HistoryClear { yes } => {
      if !yes {
        return Err(social_core::Error::input(
          "refusing to clear the browsing history without --yes",
        ));
      }
      Data::Action(history::clear(c).await?)
    }
    XhsCommand::ActiveFans { days } => Data::Users(creator::active_fans(c, days).await?),
  })
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
