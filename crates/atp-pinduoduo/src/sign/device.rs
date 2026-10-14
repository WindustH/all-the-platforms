//! The browser atp presents to the generator: Chrome on an Android phone,
//! as the H5 site expects, plus what such a browser keeps between visits
//! (the `_nano_fp` fingerprint id and the tails of earlier tokens).

use rand::Rng;
use serde::{Deserialize, Serialize};

use super::encode;

pub const USER_AGENT: &str = "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 \
  (KHTML, like Gecko) Chrome/149.0.0.0 Mobile Safari/537.36";
/// `screen.availWidth` / `availHeight` of a 412×915 phone.
pub const SCREEN: (u64, u64) = (412, 915);
/// The bot checks of `ResultInt` (`T()` plus three more bits) on Chrome
/// for Android: `navigator.plugins` is empty (bit 8) and `webdriver` is a
/// navigator property (bit 13); nothing else is set.
pub const BOT_BITS: u64 = (1 << 8) | (1 << 13);
/// `BrowserDetect` (Opera, Firefox, Safari, IE, Chrome with extensions).
pub const BROWSER_BITS: u64 = 0;
/// `canPlayType` of mp3 and H.264/AAC mp4: both `probably`.
pub const MEDIA: (u64, u64) = (2, 2);

/// Checksums of the generator's own source in chunk
/// `3636_76a163c6741de98aa0a5.js`, computed by `ec_pbc` over function
/// texts: `cacheData` (`T` and `ct`) ends every token, and the location
/// collector appends its own. They change when PDD rebuilds the chunk.
pub const CODE_SUM: [u8; 4] = [0x06, 0x77, 0xb7, 0x30];
pub const LOCATION_SUM: [u8; 4] = [0x97, 0xe9, 0x46, 0x74];

/// How many token tails `anti_pre_sig` keeps.
const TAILS: usize = 5;
const NANOID: &[u8; 64] = b"_~varfunctio0125634789bdegjhklmpqswxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// What the browser keeps in its cookie jar and `localStorage`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Device {
  /// `_nano_fp` (cookie and `localStorage` hold the same value).
  pub nano_fp: String,
  /// `anti_pre_sig`: the last 8 characters of the latest tokens.
  pub tails: Vec<String>,
}

impl Device {
  pub fn new(now_ms: i64) -> Self {
    Self {
      nano_fp: nano_fp(now_ms),
      tails: Vec::new(),
    }
  }

  /// `writePreviousAntiSig`.
  pub fn remember(&mut self, token: &str) {
    let tail: String = {
      let chars: Vec<char> = token.chars().collect();
      chars[chars.len().saturating_sub(8)..].iter().collect()
    };
    self.tails.push(tail);
    while self.tails.len() > TAILS {
      self.tails.remove(0);
    }
  }
}

/// A new `_nano_fp`: the encoded seconds and a checksum digit triple, then
/// a 21-character nanoid (module 18 of the chunk).
pub fn nano_fp(now_ms: i64) -> String {
  let mut rng = rand::rng();
  let id: String = (0..21)
    .map(|_| NANOID[rng.random_range(0..64)] as char)
    .collect();
  let seconds: String = now_ms.to_string().chars().take(10).collect();
  let sum: u32 = format!("{seconds}_{id}").chars().map(|c| c as u32).sum();
  let digits = format!("{:0<3}", sum % 1000);
  let stamp = encode::encode(format!("{seconds}{digits}").as_bytes(), None);
  format!("{stamp}_{id}")
}
