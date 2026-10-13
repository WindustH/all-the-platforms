//! What every module works with: the context with mp.weixin's preferred
//! user agent (article pages show a captcha to desktop Chrome).

use social_core::Ctx;

const SAFARI: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
   (KHTML, like Gecko) Version/18.5 Safari/605.1.15";

pub struct Client {
  pub ctx: Ctx,
}

impl Client {
  pub fn new(mut ctx: Ctx) -> Self {
    ctx.http.set_header("user-agent", SAFARI);
    for hint in ["sec-ch-ua", "sec-ch-ua-mobile", "sec-ch-ua-platform"] {
      ctx.http.set_header(hint, "");
    }
    Self { ctx }
  }
}
