//! QR login through Discord's remote auth gateway, as the login page does:
//! the app on a phone scans `https://discord.com/ra/<fingerprint>` and
//! confirms; the token comes back encrypted to a key made for this login.
//!
//! Protocol (v2): `hello` → we send `init` with an RSA-2048 public key →
//! `nonce_proof` (a nonce encrypted to it; we answer with the base64url
//! SHA-256 of the plain nonce) → `pending_remote_init` with the fingerprint
//! → `pending_ticket` once scanned (the user, encrypted) → `pending_login`
//! with a ticket once confirmed, exchanged at
//! `/users/@me/remote-auth/login` for the encrypted token. `cancel` means
//! the phone declined; the socket closes when the code expires.

use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use rsa::pkcs8::EncodePublicKey;
use rsa::rand_core::OsRng;
use rsa::{Oaep, RsaPrivateKey};
use sha2::{Digest, Sha256};
use social_core::http::Method;
use social_core::socket::Socket;
use social_core::{Error, QrStatus, Result, Value, ValueExt, json};

use crate::api::{Api, Body, Kind, SOCKET_HEADERS, Token, WEB};

const GATEWAY: &str = "wss://remote-auth-gateway.discord.gg/?v=2";
/// How long one poll waits for gateway messages.
const POLL: Duration = Duration::from_millis(400);

pub struct Pending {
  socket: Socket,
  key: RsaPrivateKey,
  heartbeat: Duration,
  last_beat: Instant,
  scanned: bool,
}

/// Open the gateway and run the handshake; the QR content comes back.
pub async fn start(api: &Api) -> Result<(Pending, String)> {
  let mut socket = api
    .ctx
    .http
    .websocket(GATEWAY, &SOCKET_HEADERS, false)
    .await?;
  let hello = next(&mut socket).await?;
  if hello.str("op").as_deref() != Some("hello") {
    return Err(Error::upstream("the remote auth gateway did not say hello"));
  }
  let heartbeat = Duration::from_millis(hello.u64("heartbeat_interval").unwrap_or(41_250));
  let key =
    RsaPrivateKey::new(&mut OsRng, 2048).map_err(|e| Error::internal(format!("RSA key: {e}")))?;
  let public = key
    .to_public_key()
    .to_public_key_der()
    .map_err(|e| Error::internal(format!("RSA key: {e}")))?;
  socket
    .send_json(&json!({"op": "init", "encoded_public_key": STANDARD.encode(public.as_bytes())}))
    .await?;
  let deadline = Instant::now() + Duration::from_secs(30);
  while Instant::now() < deadline {
    let v = next(&mut socket).await?;
    match v.str("op").as_deref() {
      Some("nonce_proof") => {
        let nonce = decrypt(&key, &v.str("encrypted_nonce").unwrap_or_default())?;
        let proof = URL_SAFE_NO_PAD.encode(Sha256::digest(&nonce));
        socket
          .send_json(&json!({"op": "nonce_proof", "proof": proof}))
          .await?;
      }
      Some("pending_remote_init") => {
        let fingerprint = v
          .str("fingerprint")
          .ok_or_else(|| Error::upstream("remote auth: no fingerprint"))?;
        let pending = Pending {
          socket,
          key,
          heartbeat,
          last_beat: Instant::now(),
          scanned: false,
        };
        return Ok((pending, format!("{WEB}/ra/{fingerprint}")));
      }
      _ => {}
    }
  }
  Err(Error::upstream("the remote auth gateway gave no QR code"))
}

/// What happened since the last poll. On confirmation the token is saved.
pub async fn poll(api: &Api, p: &mut Pending) -> Result<QrStatus> {
  if p.last_beat.elapsed() >= p.heartbeat {
    p.socket.send_json(&json!({"op": "heartbeat"})).await?;
    p.last_beat = Instant::now();
  }
  loop {
    let v = match tokio::time::timeout(POLL, p.socket.next_json()).await {
      Err(_) => return Ok(status(p)),
      Ok(Ok(None)) => return Ok(QrStatus::Expired),
      Ok(Ok(Some(v))) => v,
      Ok(Err(e)) => return Err(e),
    };
    match v.str("op").as_deref() {
      Some("pending_ticket") => {
        p.scanned = true;
        // `id:discriminator:avatar:username`; only logged at debug level.
        if let Ok(user) = decrypt(&p.key, &v.str("encrypted_user_payload").unwrap_or_default()) {
          let user = String::from_utf8_lossy(&user).into_owned();
          tracing::debug!("scanned by {}", user.rsplit(':').next().unwrap_or_default());
        }
        return Ok(QrStatus::Scanned);
      }
      Some("pending_login") => {
        let ticket = v
          .str("ticket")
          .ok_or_else(|| Error::upstream("remote auth: no ticket"))?;
        let token = exchange(api, &ticket, &p.key).await?;
        api.save_token(&token);
        return Ok(QrStatus::Confirmed);
      }
      // v1 servers send the token right away.
      Some("finish") => {
        let token = decrypt(&p.key, &v.str("encrypted_token").unwrap_or_default())?;
        api.save_token(&user_token(token)?);
        return Ok(QrStatus::Confirmed);
      }
      Some("cancel") => {
        return Err(Error::auth("the login was declined on the phone"));
      }
      _ => {}
    }
  }
}

fn status(p: &Pending) -> QrStatus {
  if p.scanned {
    QrStatus::Scanned
  } else {
    QrStatus::Waiting
  }
}

async fn exchange(api: &Api, ticket: &str, key: &RsaPrivateKey) -> Result<Token> {
  let v = api
    .send(
      None,
      Method::POST,
      "/users/@me/remote-auth/login",
      &[],
      Body::Json(json!({"ticket": ticket})),
    )
    .await?;
  let encrypted = v
    .str("encrypted_token")
    .ok_or_else(|| Error::upstream("remote auth: no token in the answer"))?;
  user_token(decrypt(key, &encrypted)?)
}

fn user_token(bytes: Vec<u8>) -> Result<Token> {
  let secret =
    String::from_utf8(bytes).map_err(|_| Error::upstream("remote auth: the token is not text"))?;
  Ok(Token {
    secret,
    kind: Kind::User,
  })
}

fn decrypt(key: &RsaPrivateKey, b64: &str) -> Result<Vec<u8>> {
  let data = STANDARD
    .decode(b64)
    .map_err(|_| Error::upstream("remote auth: bad base64"))?;
  key
    .decrypt(Oaep::new::<Sha256>(), &data)
    .map_err(|e| Error::upstream(format!("remote auth: cannot decrypt ({e})")))
}

async fn next(socket: &mut Socket) -> Result<Value> {
  match tokio::time::timeout(Duration::from_secs(20), socket.next_json()).await {
    Ok(Ok(Some(v))) => Ok(v),
    Ok(Ok(None)) => Err(Error::upstream(
      "the remote auth gateway closed the connection",
    )),
    Ok(Err(e)) => Err(e),
    Err(_) => Err(Error::network("the remote auth gateway did not answer")),
  }
}
