//! Files of received messages (`tg-bot-file:` media), fetched through
//! `getFile`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use social_core::{Ctx, Error, Media, Result, ValueExt, json};

use super::{API, Bot};

impl Bot {
  /// Fetch a `tg-bot-file:` media (at most 20 MB, a Bot API limit).
  pub async fn save(&self, ctx: &Ctx, media: &Media, dir: &Path, name: &str) -> Result<PathBuf> {
    let file_id = media
      .url
      .strip_prefix("tg-bot-file:")
      .ok_or_else(|| Error::internal("not a bot file"))?;
    let file = self
      .call(ctx, "getFile", json!({"file_id": file_id}))
      .await?;
    let path = file.str("file_path").ok_or_else(|| {
      Error::upstream("Telegram gave no file path (files over 20 MB cannot be fetched by bots)")
    })?;
    let ext = path
      .rsplit_once('.')
      .map(|(_, e)| e.to_owned())
      .unwrap_or_else(|| "bin".into());
    let out = dir.join(format!("{name}.{ext}"));
    let resp = ctx
      .http
      .get(format!("{API}/file/bot{}/{path}", self.token))
      .no_cookies()
      .timeout(Duration::from_secs(300))
      .send()
      .await?
      .check()?;
    tokio::fs::write(&out, &resp.body).await?;
    Ok(out)
  }
}
