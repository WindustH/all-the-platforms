//! QR login on Taobao's Havana passport (`login.taobao.com`, atp-alibaba's
//! [`havana`]). The login page holds the form fields every passport call
//! carries (`loginFormData`, with its CSRF token) and its trace id;
//! `qrCode/generate.do` hands out the code and `qrCode/query.do` tells how
//! it stands. A confirmed scan sets the session cookies (some through
//! cookie-setting links on the other site) and names the page the login
//! goes on to; when that page is an identity check (face or SMS), the
//! login is `verification_required` with that page.

use atp_alibaba::havana;
use atp_core::http::USER_AGENT;
use atp_core::{Error, QrStatus, QrTicket, Result, Value, ValueExt};

use crate::api::{Api, DOCUMENT, WWW};

const LOGIN: &str = "https://login.taobao.com";
const LOGIN_PAGE: &str = "https://login.taobao.com/havanaone/login/login.htm?bizName=taobao";
const GENERATE: &str = "/havanaone/loginLegacy/qrCode/generate.do";
const QUERY: &str = "/havanaone/loginLegacy/qrCode/query.do";
/// The query of every passport call the page makes.
const ENTRANCE: [(&str, &str); 2] = [("bizEntrance", "taobao_pc"), ("bizName", "taobao")];
/// Ticket extras: the page's form fields (JSON) and its trace id.
const FORM: &str = "form";
const TRACE: &str = "trace";

pub async fn qr_start(api: &Api) -> Result<QrTicket> {
  let http = &api.ctx.http;
  let html = http
    .get(LOGIN_PAGE)
    .header("accept", DOCUMENT)
    .header("referer", format!("{WWW}/"))
    .send()
    .await?
    .text();
  let form =
    form_data(&html).ok_or_else(|| Error::upstream("the Taobao login page has no login form"))?;
  let mut query = fields(&form);
  query.extend(umid());
  let v = http
    .get(format!("{LOGIN}{GENERATE}"))
    .queries(ENTRANCE)
    .queries(query)
    .header("referer", LOGIN_PAGE)
    .send()
    .await?
    .value()?;
  let code = havana::code(&v, "Taobao")?;
  Ok(QrTicket {
    url: code.content,
    token: code.t,
    extra: [
      ("ck".to_owned(), code.ck),
      (FORM.to_owned(), form.to_string()),
      (
        TRACE.to_owned(),
        meta(&html, "eagleeye-trace").unwrap_or_default(),
      ),
    ]
    .into(),
    image: None,
  })
}

pub async fn qr_poll(api: &Api, ticket: &QrTicket) -> Result<QrStatus> {
  let http = &api.ctx.http;
  let extra = |k: &str| ticket.extra.get(k).cloned().unwrap_or_default();
  let form: Value = serde_json::from_str(&extra(FORM)).unwrap_or_default();
  let mut body = vec![
    ("t", ticket.token.clone()),
    ("ck", extra("ck")),
    ("ua", String::new()),
  ];
  body.extend(fields(&form));
  body.extend(umid());
  body.extend([
    ("navlanguage", "zh-CN".into()),
    ("navUserAgent", USER_AGENT.into()),
    ("navPlatform", "Linux x86_64".into()),
    ("isIframe", "false".into()),
    ("documentReferer", format!("{WWW}/")),
    ("defaultView", "qrcode".into()),
    ("deviceId", http.cookie("cna").unwrap_or_default()),
    ("pageTraceId", extra(TRACE)),
  ]);
  let v = http
    .post(format!("{LOGIN}{QUERY}"))
    .queries(ENTRANCE)
    .form(body)
    .header("origin", LOGIN)
    .header("referer", LOGIN_PAGE)
    .send()
    .await?
    .value()?;
  let (status, data) = havana::status(&v)?;
  if status == QrStatus::Confirmed {
    confirm(api, data).await?;
  }
  Ok(status)
}

/// Collect the cookies a confirmed login hands out, unless it wants an
/// identity check first.
async fn confirm(api: &Api, data: &Value) -> Result<()> {
  let http = &api.ctx.http;
  let redirect = havana::redirect(data);
  if let Some(url) = redirect.as_deref().filter(|u| havana::is_check(u)) {
    return Err(check(Some(url.to_owned())));
  }
  for url in links(data.at("asyncUrls")).iter().filter(|u| on_site(u)) {
    let _ = http.get(url).send().await;
  }
  if let Some(url) = redirect.as_deref().filter(|u| on_site(u)) {
    let _ = http.get(url).header("accept", DOCUMENT).send().await;
  }
  if http.has_cookie("unb") {
    return Ok(());
  }
  match redirect {
    Some(url) => Err(check(Some(url))),
    None => Err(
      Error::auth("Taobao confirmed the scan but set no session")
        .with_hint("log in on www.taobao.com in a browser, then run `atp taobao login --browser`"),
    ),
  }
}

fn check(url: Option<String>) -> Error {
  havana::check_wanted("Taobao", "taobao", "www.taobao.com", url)
}

/// `window.viewData.loginFormData` of the login page.
fn form_data(html: &str) -> Option<Value> {
  let start = html.find("\"loginFormData\":")? + "\"loginFormData\":".len();
  let mut stream = serde_json::Deserializer::from_str(&html[start..]).into_iter::<Value>();
  stream.next()?.ok().filter(Value::is_object)
}

/// An object's fields as text pairs.
fn fields(form: &Value) -> Vec<(&str, String)> {
  let Some(map) = form.as_object() else {
    return Vec::new();
  };
  map
    .iter()
    .map(|(k, v)| {
      let text = v.as_str().map_or_else(|| v.to_string(), str::to_owned);
      (k.as_str(), text)
    })
    .collect()
}

/// What the page sends without the device script's token.
fn umid() -> [(&'static str, String); 2] {
  [("umidToken", String::new()), ("umidTag", "SERVER".into())]
}

/// `<meta name="…" content="…">`.
fn meta(html: &str, name: &str) -> Option<String> {
  let tag = format!("name=\"{name}\" content=\"");
  let start = html.find(&tag)? + tag.len();
  let end = html[start..].find('"')?;
  Some(html[start..start + end].to_owned())
}

/// Links as a list or as an object's values.
fn links(v: &Value) -> Vec<String> {
  match v {
    Value::Array(items) => items
      .iter()
      .filter_map(Value::as_str)
      .map(str::to_owned)
      .collect(),
    Value::Object(map) => map
      .values()
      .filter_map(Value::as_str)
      .map(str::to_owned)
      .collect(),
    _ => Vec::new(),
  }
}

/// Whether a link is on Taobao or Tmall, where the session's cookies live.
fn on_site(url: &str) -> bool {
  let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
  let host = rest.split(['/', '?', '#', ':']).next().unwrap_or_default();
  ["taobao.com", "tmall.com"]
    .iter()
    .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}
