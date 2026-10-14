//! Juejin commands beyond the shared set: a user's pins and the daily
//! check-in (签到).

use atp_core::cli::PageArgs;
use atp_core::{Action, Data, ErrorCode, Result, ValueExt, json};

use crate::api::Api;
use crate::posts;

/// `err_no` of a check-in done earlier today.
const ALREADY: &str = "15001";

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// A user's pins (沸点), the newest first
  #[command(after_long_help = EX_PINS)]
  Pins {
    /// User: link, id or #N
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// The daily check-in (签到) for 矿石 points (needs a login)
  #[command(name = "check-in", after_long_help = EX_CHECK_IN)]
  CheckIn,
}

const EX_PINS: &str = "\
Examples:
  atp juejin pins https://juejin.cn/user/4089838985816968
  atp juejin feed --type pins                  # recommended pins

More: atp guide juejin pins";

const EX_CHECK_IN: &str = "\
Examples:
  atp juejin check-in

More: atp guide juejin check-in";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Pins { user, page } => {
      let user = api.ctx.user_ref(&user)?;
      Data::Posts(
        page
          .collect_dated(async |req| posts::pins(api, &user, &req).await)
          .await?,
      )
    }
    Command::CheckIn => Data::Action(check_in(api).await?),
  })
}

async fn check_in(api: &Api) -> Result<Action> {
  match api.write("/growth_api/v1/check_in", &json!({})).await {
    Ok(v) => {
      let data = v.at("data");
      let message = format!(
        "+{} points, {} in all",
        data.u64("incr_point").unwrap_or(0),
        data.u64("sum_point").unwrap_or(0)
      );
      Ok(Action::done("check-in", "juejin").with_message(message))
    }
    Err(e) if e.code == ErrorCode::UpstreamError && e.message.contains(ALREADY) => {
      Ok(Action::done("check-in", "juejin").with_message("already checked in today"))
    }
    Err(e) => Err(e),
  }
}
