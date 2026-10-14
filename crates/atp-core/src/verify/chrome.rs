//! Finding and starting a Chromium-family browser with a throwaway profile
//! and DevTools on a local port.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::{Error, Result};

/// Executables looked up on `PATH` (Linux, BSD).
const COMMANDS: &[&str] = &[
  "google-chrome",
  "google-chrome-stable",
  "chromium",
  "chromium-browser",
  "microsoft-edge",
  "microsoft-edge-stable",
  "brave-browser",
  "brave",
];

/// App bundles under `/Applications` and `~/Applications` (macOS).
const BUNDLES: &[&str] = &[
  "Google Chrome.app/Contents/MacOS/Google Chrome",
  "Chromium.app/Contents/MacOS/Chromium",
  "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
  "Brave Browser.app/Contents/MacOS/Brave Browser",
];

/// Installs under `%ProgramFiles%`, `%ProgramFiles(x86)%` and
/// `%LocalAppData%` (Windows).
const INSTALLS: &[&str] = &[
  r"Google\Chrome\Application\chrome.exe",
  r"Microsoft\Edge\Application\msedge.exe",
  r"BraveSoftware\Brave-Browser\Application\brave.exe",
  r"Chromium\Application\chrome.exe",
];

/// The browser to drive: `ATP_BROWSER` (a path or a command), else the
/// first installed Chrome, Chromium, Edge or Brave.
pub fn find() -> Option<PathBuf> {
  if let Some(chosen) = std::env::var_os("ATP_BROWSER").filter(|v| !v.is_empty()) {
    let path = PathBuf::from(&chosen);
    return if path.is_file() {
      Some(path)
    } else {
      on_path(&chosen.to_string_lossy())
    };
  }
  candidates().into_iter().find(|p| p.is_file())
}

fn candidates() -> Vec<PathBuf> {
  if cfg!(target_os = "macos") {
    let mut roots = vec![PathBuf::from("/Applications")];
    roots.extend(dirs::home_dir().map(|h| h.join("Applications")));
    return roots
      .iter()
      .flat_map(|root| BUNDLES.iter().map(move |b| root.join(b)))
      .collect();
  }
  if cfg!(windows) {
    return ["ProgramFiles", "ProgramFiles(x86)", "LocalAppData"]
      .iter()
      .filter_map(std::env::var_os)
      .flat_map(|root| INSTALLS.iter().map(move |i| Path::new(&root).join(i)))
      .collect();
  }
  COMMANDS.iter().filter_map(|c| on_path(c)).collect()
}

/// `name` resolved against `PATH` (with `.exe` on Windows).
fn on_path(name: &str) -> Option<PathBuf> {
  let file = if cfg!(windows) && !name.ends_with(".exe") {
    format!("{name}.exe")
  } else {
    name.to_owned()
  };
  std::env::split_paths(&std::env::var_os("PATH")?)
    .map(|dir| dir.join(&file))
    .find(|p| p.is_file())
}

/// A running browser on its own profile; dropping it ends the process and
/// deletes the profile.
pub struct Browser {
  child: Child,
  profile: PathBuf,
}

impl Browser {
  /// Start `exe` on an empty profile, through `proxy` when given (Chrome
  /// takes `scheme://host:port` without credentials).
  pub fn launch(exe: &Path, proxy: Option<&str>) -> Result<Self> {
    let profile = std::env::temp_dir().join(format!("atp-verify-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&profile);
    std::fs::create_dir_all(&profile)?;
    let mut cmd = Command::new(exe);
    cmd
      .arg(format!("--user-data-dir={}", profile.display()))
      .args([
        "--remote-debugging-port=0",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-sync",
        "--window-size=1100,900",
      ]);
    if let Some(proxy) = proxy {
      cmd.arg(format!("--proxy-server={proxy}"));
    }
    let child = cmd
      .arg("about:blank")
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .map_err(|e| Error::internal(format!("cannot start {}: {e}", exe.display())))?;
    Ok(Self { child, profile })
  }

  /// The DevTools websocket of the browser, once it has written its port.
  pub async fn endpoint(&mut self) -> Result<String> {
    let file = self.profile.join("DevToolsActivePort");
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
      if let Ok(text) = std::fs::read_to_string(&file) {
        let mut lines = text.lines();
        if let (Some(port), Some(path)) = (lines.next(), lines.next()) {
          return Ok(format!("ws://127.0.0.1:{}{}", port.trim(), path.trim()));
        }
      }
      if self.exited() {
        break;
      }
      tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Err(Error::internal("the browser did not open its DevTools port"))
  }

  pub fn exited(&mut self) -> bool {
    !matches!(self.child.try_wait(), Ok(None))
  }

  /// Wait up to a few seconds for the browser to end after `Browser.close`.
  pub async fn wait(&mut self) {
    for _ in 0..25 {
      if self.exited() {
        return;
      }
      tokio::time::sleep(Duration::from_millis(200)).await;
    }
  }
}

impl Drop for Browser {
  fn drop(&mut self) {
    if !self.exited() {
      let _ = self.child.kill();
      let _ = self.child.wait();
    }
    let _ = std::fs::remove_dir_all(&self.profile);
  }
}

/// Open `url` in the default browser; `false` when no opener worked.
pub fn open_default(url: &str) -> bool {
  let mut cmd = if cfg!(target_os = "macos") {
    Command::new("open")
  } else if cfg!(windows) {
    let mut c = Command::new("rundll32");
    c.arg("url.dll,FileProtocolHandler");
    c
  } else {
    Command::new("xdg-open")
  };
  cmd
    .arg(url)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .is_ok()
}
