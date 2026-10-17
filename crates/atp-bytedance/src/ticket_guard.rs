//! bd-ticket-guard: the P-256 key pair a ByteDance web login registers with
//! the passport, and the signatures writes carry with it.
//!
//! Before logging in the client hands its public key over in the
//! `bd_ticket_guard_client_data` cookie; the passport answers the login with
//! `bd-ticket-guard-server-data` (a ticket, a `ts_sign` and a certificate
//! for that key). Writes then sign `ticket=…&path=…&timestamp=…` with the
//! private key (`bd-ticket-guard-client-data`). Shapes as `cv-cat/catbus`
//! (`src/platforms/douyin/web/crypto.ts`, MIT) reproduces them.
//!
//! Two variants: ECDSA (sign type 0), which www.douyin.com's writes carry,
//! and the HMAC one (sign type 1) of the creator platform's secure
//! consumer SDK ([`consumer_headers`]), keyed by an ECDH exchange with the
//! `ecies-server` key built into that SDK.

use atp_core::text::{decode_uri_component, encode_uri_component};
use atp_core::{Error, Result, Value, ValueExt, json};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, Mac};
use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, KeyPair};
use sha2::Sha256;

/// Header of the passport's answer holding the ticket for the key.
pub const SERVER_DATA: &str = "bd-ticket-guard-server-data";
/// The same answer, when it comes as a cookie.
pub const SERVER_DATA_COOKIE: &str = "bd_ticket_guard_server_data";
/// Cookie handing the public key to the passport.
pub const CLIENT_DATA_COOKIE: &str = "bd_ticket_guard_client_data";
/// Cookie naming the web client kind (`2`).
pub const WEB_DOMAIN_COOKIE: &str = "bd_ticket_guard_client_web_domain";

/// Public key of the `ecies-server` certificate (issued by
/// `ticket_guard_ca_ecdsa_256`, valid to 2069) that the creator pages'
/// secure consumer SDK 1.0.0.22 falls back to; its HMAC keys come from
/// ECDH with it.
const ECIES_SERVER_KEY: &str = "-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEhNpZQz5X3PEavuSeR14YOHnRYz5w
hd1GuyQaIhP36D/Zuy+RCyHL8QoBMWkQh20PJAgivwK2eBV/6gqG2QJE6A==
-----END PUBLIC KEY-----
";

/// The private key, kept as PKCS#8.
pub struct Key {
  pkcs8: Vec<u8>,
  pair: EcdsaKeyPair,
}

impl Key {
  pub fn generate() -> Result<Self> {
    let rng = SystemRandom::new();
    let doc = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng)
      .map_err(|_| Error::internal("could not create a P-256 key"))?;
    Self::from_pkcs8(doc.as_ref())
  }

  /// A key saved with [`Key::to_base64`].
  pub fn from_base64(text: &str) -> Result<Self> {
    let pkcs8 = STANDARD
      .decode(text.trim())
      .map_err(|_| Error::internal("the saved ticket-guard key is damaged"))?;
    Self::from_pkcs8(&pkcs8)
  }

  fn from_pkcs8(pkcs8: &[u8]) -> Result<Self> {
    let rng = SystemRandom::new();
    let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8, &rng)
      .map_err(|_| Error::internal("the saved ticket-guard key is damaged"))?;
    Ok(Self {
      pkcs8: pkcs8.to_vec(),
      pair,
    })
  }

  pub fn to_base64(&self) -> String {
    STANDARD.encode(&self.pkcs8)
  }

  /// The public point `04 || X || Y`, base64 (`ree-public-key`).
  pub fn public(&self) -> String {
    STANDARD.encode(self.pair.public_key().as_ref())
  }

  /// ECDSA-SHA256 of `message`, DER, base64.
  fn sign(&self, message: &str) -> Result<String> {
    let signature = self
      .pair
      .sign(&SystemRandom::new(), message.as_bytes())
      .map_err(|_| Error::internal("could not sign with the ticket-guard key"))?;
    Ok(STANDARD.encode(signature.as_ref()))
  }

  /// The consumer SDK's HMAC key: HKDF-SHA256 (empty salt and info, 32
  /// bytes) of the ECDH secret between this key and the ecies server key.
  pub fn hmac_key(&self) -> Result<[u8; 32]> {
    use btls::derive::Deriver;
    use btls::pkey::PKey;
    let fail = |e: btls::error::ErrorStack| Error::internal(format!("ticket-guard ECDH: {e}"));
    let private = PKey::private_key_from_pkcs8(&self.pkcs8).map_err(fail)?;
    let peer = PKey::public_key_from_pem(ECIES_SERVER_KEY.as_bytes()).map_err(fail)?;
    let mut deriver = Deriver::new(&private).map_err(fail)?;
    deriver.set_peer(&peer).map_err(fail)?;
    let secret = deriver.derive_to_vec().map_err(fail)?;
    let prk = hmac_sha256(&[0u8; 32], &secret);
    Ok(hmac_sha256(&prk, &[1]))
  }

  /// The `bd_ticket_guard_client_data` cookie: the public key for the passport.
  pub fn client_data_cookie(&self) -> String {
    let payload = json!({
      "bd-ticket-guard-version": 2,
      "bd-ticket-guard-iteration-version": 1,
      "bd-ticket-guard-ree-public-key": self.public(),
      "bd-ticket-guard-web-version": 2,
    });
    encode_uri_component(&STANDARD.encode(payload.to_string()))
  }
}

/// What the passport issued for the key at login.
#[derive(Debug, Clone, Default)]
pub struct Ticket {
  pub ticket: String,
  pub ts_sign: String,
  pub client_cert: String,
}

impl Ticket {
  /// The `bd-ticket-guard-server-data` header or cookie: base64 JSON,
  /// percent-encoded as a cookie.
  pub fn from_server_data(raw: &str) -> Option<Self> {
    let decoded = STANDARD.decode(decode_uri_component(raw.trim())).ok()?;
    let v: Value = serde_json::from_slice(&decoded).ok()?;
    Some(Self {
      ticket: v.str("ticket").filter(|t| !t.is_empty())?,
      ts_sign: v.str("ts_sign").unwrap_or_default(),
      client_cert: v.str("client_cert").unwrap_or_default(),
    })
  }

  /// `bd-ticket-guard-web-version`: 1 for `ts.1…` signs, 2 otherwise.
  fn web_version(&self) -> &'static str {
    if self.ts_sign.starts_with("ts.1") {
      "1"
    } else {
      "2"
    }
  }
}

/// Headers naming the key on writes that carry no signature (likes,
/// saves, follows).
pub fn key_headers(key: &Key, ticket: &Ticket) -> Vec<(&'static str, String)> {
  let sign_type = if ticket.client_cert.starts_with("pub.") {
    "1"
  } else {
    "0"
  };
  vec![
    ("bd-ticket-guard-ree-public-key", key.public()),
    ("bd-ticket-guard-version", "2".into()),
    ("bd-ticket-guard-web-version", ticket.web_version().into()),
    ("bd-ticket-guard-web-sign-type", sign_type.into()),
  ]
}

/// Headers of a signed write to `path` at `now` (unix seconds): the
/// ECDSA signature of ticket, path and time, plus the key.
pub fn write_headers(
  key: &Key,
  ticket: &Ticket,
  path: &str,
  now: i64,
) -> Result<Vec<(&'static str, String)>> {
  let content = format!("ticket={}&path={path}&timestamp={now}", ticket.ticket);
  let payload = json!({
    "ts_sign": ticket.ts_sign,
    "req_content": "ticket,path,timestamp",
    "req_sign": key.sign(&content)?,
    "timestamp": now,
  });
  Ok(vec![
    (
      "bd-ticket-guard-client-data",
      STANDARD.encode(payload.to_string()),
    ),
    ("bd-ticket-guard-ree-public-key", key.public()),
    ("bd-ticket-guard-version", "2".into()),
    ("bd-ticket-guard-web-version", ticket.web_version().into()),
    ("bd-ticket-guard-web-sign-type", "0".into()),
  ])
}

/// Headers of a write the creator platform's secure consumer SDK guards
/// (`create_v2`, `delete` ...), in its order: the HMAC of ticket, path and
/// `timestamp` under [`Key::hmac_key`] (sign type 1), or ECDSA (type 0)
/// when the exchange fails, as the SDK falls back. `trusted` marks a
/// timestamp the passport handed out (`bd-ticket-guard-ts`), as `t_trust`.
pub fn consumer_headers(
  key: &Key,
  ticket: &Ticket,
  path: &str,
  timestamp: i64,
  trusted: bool,
) -> Result<Vec<(&'static str, String)>> {
  let content = format!("ticket={}&path={path}&timestamp={timestamp}", ticket.ticket);
  let (req_sign, sign_type) = match key.hmac_key() {
    Ok(hmac_key) => (
      STANDARD.encode(hmac_sha256(&hmac_key, content.as_bytes())),
      "1",
    ),
    Err(e) => {
      tracing::debug!("{e}; signing with ECDSA");
      (key.sign(&content)?, "0")
    }
  };
  let mut payload = json!({
    "ts_sign": ticket.ts_sign,
    "req_content": "ticket,path,timestamp",
    "req_sign": req_sign,
    "timestamp": timestamp,
  });
  if trusted {
    payload["t_trust"] = json!(1);
  }
  Ok(vec![
    ("bd-ticket-guard-ree-public-key", key.public()),
    ("bd-ticket-guard-web-version", ticket.web_version().into()),
    ("bd-ticket-guard-web-sign-type", sign_type.into()),
    ("bd-ticket-guard-version", "2".into()),
    (
      "bd-ticket-guard-client-data",
      STANDARD.encode(payload.to_string()),
    ),
  ])
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
  let mut mac = <Hmac<Sha256>>::new_from_slice(key).expect("HMAC takes any key");
  mac.update(data);
  mac.finalize().into_bytes().into()
}
