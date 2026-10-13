//! `listen`: messages (and edits, deletions, reactions) as they arrive.

use std::time::Duration;

use social_core::Result;
use social_core::output::{Stream, note};
use whatsapp_rust::prelude::Event;

use crate::link::Link;
use crate::personal::not_linked;
use crate::{parse, refs};

/// Print messages (and edits, deletions, reactions) as they arrive.
pub async fn listen(link: &mut Link, chats: &[String], out: &Stream) -> Result<()> {
  let me = link.me();
  let wanted: Vec<String> = chats
    .iter()
    .map(|c| refs::chat(c, &link.archive, &me))
    .collect::<Result<_>>()?;
  note("Listening; press Ctrl-C to stop.");
  loop {
    let event = tokio::select! {
      _ = tokio::signal::ctrl_c() => break,
      event = link.next(Duration::from_secs(10)) => event,
    };
    if let Some(event) = event {
      if let Event::LoggedOut(_) = &*event {
        return Err(not_linked());
      }
      for (record, what) in link.absorb(&event) {
        if !wanted.is_empty() && !wanted.contains(&record.chat) {
          continue;
        }
        let mut post = parse::post(&record, &link.archive);
        if what != "message" {
          post.kind = what.into();
        }
        out.post(&post);
      }
    }
    link.save_every(Duration::from_secs(10)).await?;
  }
  Ok(())
}
