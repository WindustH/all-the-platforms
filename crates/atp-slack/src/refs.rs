//! Message and conversation arguments: permalinks
//! (`https://acme.slack.com/archives/C1/p1700000000000100?thread_ts=…`),
//! `CHANNEL/TS` (the id of listed messages), and conversation ids or links.

use atp_core::chat;
use atp_core::{Error, Result};
use url::Url;

pub struct MessageRef {
  pub channel: String,
  pub ts: String,
  /// The thread the message is a reply in, when the link says.
  pub thread_ts: Option<String>,
}

impl MessageRef {
  /// The `CHANNEL/TS` id listed messages carry.
  pub fn id(&self) -> String {
    chat::message_id(&self.channel, &self.ts)
  }

  /// The thread a reply to this message goes in: its own, or itself.
  pub fn root(&self) -> &str {
    self.thread_ts.as_deref().unwrap_or(&self.ts)
  }
}

pub fn message(arg: &str) -> Result<MessageRef> {
  let arg = arg.trim();
  if let Ok(url) = Url::parse(arg) {
    let segs: Vec<&str> = url.path_segments().map(|s| s.collect()).unwrap_or_default();
    if let Some(at) = segs.iter().position(|s| *s == "archives")
      && let (Some(channel), Some(p)) = (segs.get(at + 1), segs.get(at + 2))
    {
      let thread_ts = url
        .query_pairs()
        .find(|(k, _)| k == "thread_ts")
        .map(|(_, v)| v.into_owned());
      return Ok(MessageRef {
        channel: (*channel).to_owned(),
        ts: ts_of(p)?,
        thread_ts,
      });
    }
    return Err(Error::input(format!("not a Slack message link: {arg}")));
  }
  if let Some((channel, ts)) = chat::split_message_id(arg)
    && is_chat_id(channel)
  {
    return Ok(MessageRef {
      channel: channel.to_owned(),
      ts: ts_of(ts)?,
      thread_ts: None,
    });
  }
  Err(Error::input(format!(
    "not a Slack message: {arg} (expected a message link or CHANNEL/TS)"
  )))
}

/// `1700000000.000100`, or the `p1700000000000100` form of links.
fn ts_of(text: &str) -> Result<String> {
  let digits = text.trim_start_matches('p');
  if digits.contains('.') {
    return Ok(digits.to_owned());
  }
  if digits.len() > 6 && digits.bytes().all(|b| b.is_ascii_digit()) {
    let (secs, micros) = digits.split_at(digits.len() - 6);
    return Ok(format!("{secs}.{micros}"));
  }
  Err(Error::input(format!(
    "not a Slack message timestamp: {text}"
  )))
}

/// `C…` (channels), `G…` (private / group), `D…` (direct) ids.
pub fn is_chat_id(id: &str) -> bool {
  matches!(id.chars().next(), Some('C' | 'G' | 'D'))
    && id.len() >= 9
    && id
      .bytes()
      .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// The conversation id in an archives or app link.
pub fn chat_link(arg: &str) -> Option<String> {
  let url = Url::parse(arg.trim()).ok()?;
  let segs: Vec<&str> = url.path_segments()?.collect();
  segs
    .iter()
    .skip_while(|s| **s != "archives" && **s != "client")
    .skip(1)
    .find(|s| is_chat_id(s))
    .map(|s| (*s).to_owned())
}
