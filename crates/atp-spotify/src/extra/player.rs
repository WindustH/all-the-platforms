//! `player status|devices|play|pause|next|previous|seek|volume|shuffle|repeat|queue|transfer`.

use atp_core::http::Method;
use atp_core::{Action, Data, Page, Result, Value};

use crate::api::Api;
use crate::player;
use crate::refs::{self, Item, Kind};

#[derive(Debug, clap::Subcommand)]
pub enum PlayerCommand {
  /// What is playing, where, how far in, shuffle and repeat
  Status,
  /// Your devices Spotify Connect sees (`#N` for --device and transfer)
  Devices,
  /// Start or resume playback: an album, playlist, artist or show, or tracks and episodes
  #[command(after_long_help = EX_PLAY)]
  Play {
    /// What to play: links, spotify: URIs, `kind:ID`, track ids or #N; resumes when omitted
    #[arg(value_name = "ITEM")]
    items: Vec<String>,
    #[command(flatten)]
    device: DeviceArg,
  },
  /// Pause playback
  Pause {
    #[command(flatten)]
    device: DeviceArg,
  },
  /// Skip to the next track
  Next {
    #[command(flatten)]
    device: DeviceArg,
  },
  /// Go back to the previous track
  Previous {
    #[command(flatten)]
    device: DeviceArg,
  },
  /// Jump to a position in the current track
  #[command(after_long_help = EX_SEEK)]
  Seek {
    /// Position: seconds, M:SS or H:MM:SS
    position: String,
    #[command(flatten)]
    device: DeviceArg,
  },
  /// Set the volume
  Volume {
    /// Volume in percent, 0 to 100
    #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
    percent: u8,
    #[command(flatten)]
    device: DeviceArg,
  },
  /// Turn shuffle on or off
  Shuffle {
    /// `on` or `off`
    #[arg(value_parser = ["on", "off"])]
    state: String,
    #[command(flatten)]
    device: DeviceArg,
  },
  /// Repeat the track, the context (album, playlist …) or nothing
  Repeat {
    /// `track`, `context` or `off`
    #[arg(value_parser = ["track", "context", "off"])]
    state: String,
    #[command(flatten)]
    device: DeviceArg,
  },
  /// Show the queue, or add a track or episode to it
  #[command(after_long_help = EX_QUEUE)]
  Queue {
    /// Track or episode to add (link, spotify: URI, `episode:ID`, track id or #N); shows the
    /// queue when omitted
    item: Option<String>,
    #[command(flatten)]
    device: DeviceArg,
  },
  /// Move playback to another device
  #[command(after_long_help = EX_TRANSFER)]
  Transfer {
    /// Device: #N from `player devices`, its id or its name
    device: String,
    /// Start playing there (else it keeps playing or paused as it was)
    #[arg(long)]
    play: bool,
  },
}

#[derive(Debug, clap::Args)]
pub struct DeviceArg {
  /// Device to control: #N from `player devices`, its id or its name; the active one when omitted
  #[arg(short, long)]
  device: Option<String>,
}

/// One command to the player, ready to send.
struct Call {
  name: &'static str,
  method: Method,
  path: &'static str,
  query: Vec<(&'static str, String)>,
  body: Option<Value>,
  detail: Option<String>,
}

impl Call {
  fn new(name: &'static str, method: Method, path: &'static str) -> Self {
    Self {
      name,
      method,
      path,
      query: Vec::new(),
      body: None,
      detail: None,
    }
  }

  fn arg(mut self, key: &'static str, value: String, detail: String) -> Self {
    self.query.push((key, value));
    self.detail = Some(detail);
    self
  }
}

pub async fn run(api: &Api, command: PlayerCommand) -> Result<Data> {
  use PlayerCommand as P;
  let (call, device) = match command {
    P::Status => return Ok(Data::Value(player::status(api).await?)),
    P::Devices => return Ok(Data::Collections(Page::last(player::devices(api).await?))),
    P::Queue { item: None, .. } => return Ok(Data::Posts(Page::last(player::queue(api).await?))),
    P::Transfer { device, play } => {
      return Ok(Data::Action(player::transfer(api, &device, play).await?));
    }
    P::Play { items, device } => (play(api, &items).await?, device),
    P::Pause { device } => (Call::new("pause", Method::PUT, "/pause"), device),
    P::Next { device } => (Call::new("next", Method::POST, "/next"), device),
    P::Previous { device } => (Call::new("previous", Method::POST, "/previous"), device),
    P::Seek { position, device } => {
      let ms = player::position(&position)?;
      let call = Call::new("seek", Method::PUT, "/seek");
      (call.arg("position_ms", ms.to_string(), position), device)
    }
    P::Volume { percent, device } => {
      let call = Call::new("volume", Method::PUT, "/volume");
      (
        call.arg("volume_percent", percent.to_string(), format!("{percent}%")),
        device,
      )
    }
    P::Shuffle { state, device } => {
      let on = (state == "on").to_string();
      (
        Call::new("shuffle", Method::PUT, "/shuffle").arg("state", on, state),
        device,
      )
    }
    P::Repeat { state, device } => {
      let call = Call::new("repeat", Method::PUT, "/repeat");
      (call.arg("state", state.clone(), state), device)
    }
    P::Queue {
      item: Some(item),
      device,
    } => (queue(api, &item).await?, device),
  };
  send(api, call, device.device.as_deref()).await
}

async fn send(api: &Api, call: Call, device: Option<&str>) -> Result<Data> {
  player::control(api, call.method, call.path, call.query, device, call.body).await?;
  let mut action = Action::done(
    format!("player {}", call.name),
    device.unwrap_or("active device"),
  );
  action.message = call.detail;
  Ok(Data::Action(action))
}

async fn play(api: &Api, args: &[String]) -> Result<Call> {
  let mut items = Vec::with_capacity(args.len());
  for arg in args {
    items.push(refs::item(api, &api.ctx.post_ref(arg)?, Kind::Track).await?);
  }
  let mut call = Call::new("play", Method::PUT, "/play");
  call.body = player::play_body(&items)?;
  if !items.is_empty() {
    call.detail = Some(items.iter().map(Item::uri).collect::<Vec<_>>().join(" "));
  }
  Ok(call)
}

async fn queue(api: &Api, arg: &str) -> Result<Call> {
  let kinds = [Kind::Track, Kind::Episode];
  let item = refs::expect(api, &api.ctx.post_ref(arg)?, Kind::Track, &kinds).await?;
  let call = Call::new("queue", Method::POST, "/queue");
  Ok(call.arg("uri", item.uri(), item.uri()))
}

pub const EX: &str = "\
Examples:
  atp spotify player status
  atp spotify player devices
  atp spotify player play spotify:album:4aawyAB9vmqN3uQ7FjRGTy --device '#2'
  atp spotify player pause

Needs Premium and a device with Spotify open. More: atp guide spotify player";

const EX_PLAY: &str = "\
Examples:
  atp spotify player play                          # resume
  atp spotify player play https://open.spotify.com/album/4aawyAB9vmqN3uQ7FjRGTy
  atp spotify search \"bohemian rhapsody\" -n 3 && atp spotify player play '#1' '#2'
  atp spotify player play playlist:3cEYpjA9oz9GiPac4AsH4n -d Kitchen

More: atp guide spotify player play";

const EX_SEEK: &str = "\
Examples:
  atp spotify player seek 90
  atp spotify player seek 2:15

More: atp guide spotify player";

const EX_QUEUE: &str = "\
Examples:
  atp spotify player queue                         # what plays next
  atp spotify player queue spotify:track:4uLU6hMCjMI75M1A2tKUQC
  atp spotify player queue '#3'

More: atp guide spotify player queue";

const EX_TRANSFER: &str = "\
Examples:
  atp spotify player devices
  atp spotify player transfer '#2' --play
  atp spotify player transfer \"Living Room\"

More: atp guide spotify player transfer";
