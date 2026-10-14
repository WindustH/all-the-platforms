//! Account commands: login, logout, status, whoami.

use std::process::ExitCode;

use clap::Subcommand;

use super::command::Outcome;
use crate::account::{self, Loaded};
use crate::error::Result;
use crate::model::Data;
use crate::platform::Platform;

#[derive(Debug, clap::Args)]
#[group(multiple = false)]
pub struct LoginArgs {
  /// Scan a QR code with the mobile app (default when the platform supports it)
  #[arg(long)]
  pub qrcode: bool,
  /// Import cookies from a local browser; all known browsers when no name is given
  #[arg(long, num_args = 0..=1, default_missing_value = "auto", value_name = "BROWSER")]
  pub browser: Option<String>,
  /// Use a cookie header copied from the browser's developer tools
  /// (`name=value; ...`; `-` reads it from stdin)
  #[arg(long, value_name = "COOKIES")]
  pub cookie: Option<String>,
  /// Use an access token or API credentials (`-` reads it from stdin)
  #[arg(long, value_name = "TOKEN")]
  pub token: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum AccountCommand {
  /// Log in and save the session
  #[command(after_long_help = EX_LOGIN)]
  Login(LoginArgs),
  /// Forget the saved session
  Logout,
  /// Check whether the saved session still works (exit code 1 when not)
  Status,
  /// Show the logged-in account
  Whoami,
}

/// Login and logout save or remove the session themselves, so their
/// outcome is never saved again afterwards.
pub(super) async fn run<P: Platform>(
  p: &P,
  cmd: AccountCommand,
  loaded: &Loaded,
) -> Result<Outcome> {
  Ok(match cmd {
    AccountCommand::Login(args) => Outcome::unsaved(account::login(p, args).await?),
    AccountCommand::Logout => Outcome::unsaved(account::logout::<P>(p.ctx())?),
    AccountCommand::Status => {
      let (data, code) = account::status(p, loaded).await?;
      Outcome {
        data,
        code,
        persist: true,
      }
    }
    AccountCommand::Whoami => Outcome::saved(Data::User(Box::new(p.whoami().await?))),
  })
}

impl Outcome {
  fn unsaved(data: Data) -> Self {
    Self {
      data,
      code: ExitCode::SUCCESS,
      persist: false,
    }
  }
}

const EX_LOGIN: &str = "\
Examples:
  atp bili login                      # QR code for the Bilibili app
  atp x login --browser               # reuse a logged-in browser
  atp x login --browser firefox
  atp zhihu login --cookie 'z_c0=...; _xsrf=...'

More: atp guide login";
