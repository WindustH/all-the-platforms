//! Accounts: yours, a contact's profile, and which numbers are on WhatsApp.

use atp_core::{Error, Page, Result, User};
use whatsapp_rust::Jid;

use crate::api::{Api, Mode, not_logged_in};
use crate::archive::{bare, kind_of};
use crate::link::Link;
use crate::refs::jid;
use crate::{parse, refs};

/// The linked account, or the Cloud API's business number.
pub async fn me(api: &Api) -> Result<User> {
  match api.mode() {
    Mode::Personal => {
      api
        .personal
        .with(&api.ctx, async |link| Ok(whoami(link)))
        .await
    }
    Mode::Cloud(cloud) => cloud.whoami(&api.ctx).await,
    Mode::None => Err(not_logged_in()),
  }
}

pub fn whoami(link: &Link) -> User {
  let me = link.me();
  User {
    id: me.clone(),
    name: Some(link.client.push_name())
      .filter(|n| !n.is_empty())
      .unwrap_or_else(|| parse::phone(&me)),
    handle: Some(parse::phone(&me)),
    ..User::default()
  }
}

pub async fn user(link: &mut Link, arg: &str) -> Result<User> {
  let me = link.me();
  let id = refs::chat(arg, &link.archive, &me)?;
  let j = jid(&id)?;
  if kind_of(&id) == "group" {
    return group_profile(link, id, &j).await;
  }
  let registered = link
    .client
    .contacts()
    .is_on_whatsapp(std::slice::from_ref(&j))
    .await
    .map_err(|e| Error::upstream(format!("lookup failed: {e}")))?
    .first()
    .is_some_and(|r| r.is_registered);
  if !registered {
    return Err(Error::not_found(format!(
      "{} is not on WhatsApp",
      parse::phone(&id)
    )));
  }
  contact_profile(link, &id, &j).await
}

/// A group as a profile: subject, description, member count.
async fn group_profile(link: &mut Link, id: String, j: &Jid) -> Result<User> {
  let meta = link
    .client
    .groups()
    .get_metadata(j)
    .await
    .map_err(|e| Error::upstream(format!("group info failed: {e}")))?;
  let mut u = User {
    id,
    name: meta.subject.clone(),
    bio: meta.description.clone(),
    ..User::default()
  };
  u.stats.followers = Some(meta.participants.len() as u64);
  Ok(u)
}

/// A registered contact: name, status text, picture, business flag.
async fn contact_profile(link: &mut Link, id: &str, j: &Jid) -> Result<User> {
  let contacts = link.client.contacts();
  let info = contacts.get_user_info(std::slice::from_ref(j)).await.ok();
  let info = info.as_ref().and_then(|m| m.values().next());
  let picture = contacts.get_profile_picture(j, false).await.ok().flatten();
  let mut u = User {
    id: bare(id),
    name: link
      .archive
      .name(id)
      .map(str::to_owned)
      .unwrap_or_else(|| parse::phone(id)),
    handle: Some(parse::phone(id)),
    bio: info.and_then(|i| i.status.clone()),
    avatar: picture.map(|p| p.url),
    ..User::default()
  };
  if let Some(i) = info {
    u.extra.insert("business".into(), i.is_business.into());
    u.extra
      .insert("devices".into(), (i.devices.len() as u64).into());
  }
  Ok(u)
}

/// Which numbers are on WhatsApp.
pub async fn check(link: &mut Link, numbers: &[String]) -> Result<Page<User>> {
  let jids: Vec<Jid> = numbers
    .iter()
    .map(|n| {
      refs::phone(n)
        .ok_or_else(|| Error::input(format!("not a phone number: {n}")))
        .and_then(|d| jid(&format!("{d}@s.whatsapp.net")))
    })
    .collect::<Result<_>>()?;
  let results = link
    .client
    .contacts()
    .is_on_whatsapp(&jids)
    .await
    .map_err(|e| Error::upstream(format!("lookup failed: {e}")))?;
  let users = results
    .iter()
    .map(|r| {
      let id = r.pn_jid.as_ref().unwrap_or(&r.jid).to_string();
      let mut u = User {
        name: parse::phone(&id),
        handle: Some(parse::phone(&id)),
        id,
        ..User::default()
      };
      u.extra.insert("registered".into(), r.is_registered.into());
      u.extra.insert("business".into(), r.is_business.into());
      u
    })
    .collect();
  Ok(Page::new(users, None))
}
