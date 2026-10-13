//! The business behind the Cloud API: its number, profile, numbers,
//! templates, and media by id.

use atp_core::{Collection, Ctx, Error, Media, MediaKind, Page, Result, User, Value, ValueExt};

use super::Cloud;

impl Cloud {
  pub async fn whoami(&self, ctx: &Ctx) -> Result<User> {
    let v = self
      .get(
        ctx,
        &self.phone,
        &[(
          "fields",
          "display_phone_number,verified_name,quality_rating,code_verification_status,name_status,platform_type,throughput,messaging_limit_tier".into(),
        )],
      )
      .await?;
    let mut u = User {
      id: self.phone.clone(),
      name: v.str("verified_name").unwrap_or_default(),
      handle: v.str("display_phone_number"),
      ..User::default()
    };
    for (k, path) in [
      ("quality", "quality_rating"),
      ("verification", "code_verification_status"),
      ("name_status", "name_status"),
      ("platform", "platform_type"),
      ("throughput", "throughput.level"),
      ("messaging_limit", "messaging_limit_tier"),
    ] {
      if let Some(s) = v.str(path) {
        u.extra.insert(k.into(), s.into());
      }
    }
    u.extra.insert("mode".into(), "cloud-api".into());
    Ok(u)
  }

  /// A media id (from a webhook payload) as a downloadable media.
  pub async fn media(&self, ctx: &Ctx, media_id: &str) -> Result<Media> {
    let v = self.get(ctx, media_id, &[]).await?;
    let url = v
      .str("url")
      .ok_or_else(|| Error::not_found("no such media (Cloud API media live 30 days)"))?;
    let mime = v.str("mime_type").unwrap_or_default();
    let kind = match mime.split('/').next() {
      Some("image") => MediaKind::Image,
      Some("video") => MediaKind::Video,
      Some("audio") => MediaKind::Audio,
      _ => MediaKind::File,
    };
    let mut m = Media::new(kind, url);
    m.size = v.u64("file_size");
    m.headers = vec![("authorization".into(), format!("Bearer {}", self.token))];
    Ok(m)
  }

  pub async fn profile(&self, ctx: &Ctx) -> Result<Value> {
    let v = self
      .get(
        ctx,
        &format!("{}/whatsapp_business_profile", self.phone),
        &[(
          "fields",
          "about,address,description,email,profile_picture_url,websites,vertical".into(),
        )],
      )
      .await?;
    Ok(v.at("data.0").clone())
  }

  pub async fn numbers(&self, ctx: &Ctx) -> Result<Page<User>> {
    let v = self
      .get(
        ctx,
        &format!("{}/phone_numbers", self.waba()?),
        &[(
          "fields",
          "id,display_phone_number,verified_name,quality_rating,code_verification_status,name_status,messaging_limit_tier".into(),
        )],
      )
      .await?;
    let users = v
      .list("data")
      .iter()
      .map(|n| {
        let mut u = User {
          id: n.str("id").unwrap_or_default(),
          name: n.str("verified_name").unwrap_or_default(),
          handle: n.str("display_phone_number"),
          ..User::default()
        };
        for (k, p) in [
          ("quality", "quality_rating"),
          ("verification", "code_verification_status"),
          ("name_status", "name_status"),
          ("messaging_limit", "messaging_limit_tier"),
        ] {
          if let Some(s) = n.str(p) {
            u.extra.insert(k.into(), s.into());
          }
        }
        u
      })
      .collect();
    Ok(Page::new(users, None))
  }

  pub async fn templates(&self, ctx: &Ctx) -> Result<Page<Collection>> {
    let v = self
      .get(
        ctx,
        &format!("{}/message_templates", self.waba()?),
        &[
          (
            "fields",
            "id,name,status,category,language,components,quality_score".into(),
          ),
          ("limit", "200".into()),
        ],
      )
      .await?;
    let items = v
      .list("data")
      .iter()
      .map(|t| {
        let body = t
          .list("components")
          .iter()
          .find(|c| c.str("type").as_deref() == Some("BODY"))
          .and_then(|c| c.str("text"));
        let mut c = Collection {
          id: t.str("id").unwrap_or_default(),
          kind: "template".into(),
          name: t.str("name").unwrap_or_default(),
          description: body,
          raw: Some(t.clone()),
          ..Collection::default()
        };
        for (k, p) in [
          ("status", "status"),
          ("category", "category"),
          ("language", "language"),
          ("quality", "quality_score.score"),
        ] {
          if let Some(s) = t.str(p) {
            c.extra.insert(k.into(), s.into());
          }
        }
        c
      })
      .collect();
    Ok(Page::new(items, None))
  }
}
