//! Accounts and shops: `whoami` asks passport whether the session is
//! logged in; `user SHOP` reads a shop's home page on `mall.jd.com`;
//! `user-posts SHOP` lists a shop's items the way the mobile shop site's
//! "all items" tab does (`whx_searchWare`, h5st-signed, with a query
//! message encrypted under a random key).

use aes::Aes128;
use atp_core::{Error, Page, PageReq, Post, Result, User, Value, ValueExt, json};
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockEncryptMut, KeyIvInit};
use md5::{Digest, Md5};
use rand::Rng;

use crate::api::{Api, Call, From, WWW, shop_url};
use crate::sign::crypto::plain_hex;
use crate::{parse, refs};

const SESSION_CHECK: &str =
  "https://passport.jd.com/loginservice.aspx?method=Login&callback=jsonpLogin";
const SHOP_ORIGIN: &str = "https://shop.m.jd.com";
/// The mobile shop site's h5st app id, color `appid` and client version.
const SHOP_SIGN_APP: &str = "ea491";
const SHOP_APPID: &str = "shop_m_jd_com";
const SHOP_CLIENT_VERSION: &str = "11.0.0";
const SHOP_PAGE: u64 = 20;
/// Parameter order of the shop site's calls.
const SHOP_ORDER: &[&str] = &[
  "functionId",
  "body",
  "t",
  "appid",
  "clientVersion",
  "client",
  "area",
  "screen",
  "uuid",
  "xAPIClientLanguage",
  "h5st",
];
const KEY_CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

pub async fn whoami(api: &Api) -> Result<User> {
  api.require_login()?;
  let text = api
    .ctx
    .http
    .get(SESSION_CHECK)
    .header("accept", "*/*")
    .header("referer", format!("{WWW}/"))
    .header("sec-fetch-dest", "script")
    .send()
    .await?
    .check()?
    .text();
  let json = text
    .split_once('(')
    .and_then(|(_, rest)| rest.rsplit_once(')'))
    .map_or(text.as_str(), |(inner, _)| inner);
  let v: Value = serde_json::from_str(json)?;
  let identity = v.at("Identity");
  if identity.bool("IsAuthenticated") != Some(true) {
    return Err(Error::auth("the JD session has expired").with_hint(api.ctx.login_hint()));
  }
  let pin = api.pin().unwrap_or_default();
  Ok(User {
    name: identity
      .first_str(&["Unick", "Name"])
      .unwrap_or_else(|| pin.clone()),
    handle: Some(pin.clone()).filter(|p| !p.is_empty()),
    url: Some("https://home.jd.com/".into()),
    id: pin,
    raw: Some(v.clone()),
    ..User::default()
  })
}

/// A shop: its name, ids and main category.
pub async fn user(api: &Api, user: &str) -> Result<User> {
  let id = refs::shop(user)?;
  let html = api.page(&shop_url(&id)).await?;
  parse::shop_page(&html, &id).ok_or_else(|| Error::not_found(format!("JD has no shop {id}")))
}

/// The items a shop sells, 20 a page, in the shop's own order.
pub async fn posts(api: &Api, user: &str, page: &PageReq) -> Result<Page<Post>> {
  let shop = refs::shop(user)?;
  let number = page.number_or(1);
  let (key, message) = query_message(&shop);
  let body = json!({
    "shopId": shop, "mtest": "", "pvid": "", "pageIdx": number, "pageSize": SHOP_PAGE,
    "sourceRpc": "shop_app_allware_list", "queryKey": key, "queryMessage": message,
    "markTag": "1", "searchType": "3", "keyWord": "", "pageId": "Shop_AllProducts",
  });
  let v = api
    .call(Call {
      function: "whx_searchWare",
      appid: SHOP_APPID,
      path: "/client.action",
      sign: Some(SHOP_SIGN_APP),
      body: Some(body.to_string()),
      params: vec![
        ("client", "pc".into()),
        ("clientVersion", SHOP_CLIENT_VERSION.into()),
        ("area", api.area()),
        ("screen", "1920*1080".into()),
        ("xAPIClientLanguage", "zh_CN".into()),
      ],
      from: From {
        origin: SHOP_ORIGIN,
        referer: format!("{SHOP_ORIGIN}/?shopId={shop}"),
        page: None,
      },
      form: true,
      time: true,
      order: SHOP_ORDER,
    })
    .await?;
  let result = ["result", "data"]
    .iter()
    .map(|k| v.at(k))
    .find(|r| r.is_object())
    .unwrap_or(&v);
  let seller = User {
    url: Some(shop_url(&shop)),
    id: shop,
    ..User::default()
  };
  let items: Vec<Post> = result
    .list("wareInfo")
    .iter()
    .filter_map(parse::ware)
    .map(|mut post| {
      post.author.get_or_insert_with(|| seller.clone());
      post
    })
    .collect();
  let more = match result.bool("hasNext") {
    Some(more) => more,
    None => result.u64("totalPage").is_some_and(|last| number < last),
  };
  let next = (more && !items.is_empty()).then(|| (number + 1).to_string());
  Ok(Page::new(items, next))
}

/// `queryKey` and `queryMessage` of the shop search: a random key, and
/// `<shop>|<ms>|<key>` AES-CBC-encrypted under it (the IV being part of
/// the key's MD5), as hex.
fn query_message(shop: &str) -> (String, String) {
  let mut rng = rand::rng();
  let key: String = (0..16)
    .map(|_| KEY_CHARS[rng.random_range(0..KEY_CHARS.len())] as char)
    .collect();
  let iv = plain_hex(&Md5::digest(key.as_bytes()))[2..18].to_owned();
  let now = jiff::Timestamp::now().as_millisecond();
  let text = format!("{shop}|{now}|{key}");
  let mut buf = text.as_bytes().to_vec();
  buf.resize(text.len() + 16, 0);
  let sealed = cbc::Encryptor::<Aes128>::new_from_slices(key.as_bytes(), iv.as_bytes())
    .expect("a 16-byte key and IV")
    .encrypt_padded_mut::<Pkcs7>(&mut buf, text.len())
    .expect("room for the padding");
  let message = plain_hex(sealed);
  (key, message)
}
