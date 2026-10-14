//! Questions a login asks on the terminal: the phone number, the code, and
//! the two-step verification password (read without echo).

use std::io::{BufRead, IsTerminal, Write};

use atp_core::{Error, Result};

/// One line from the terminal.
pub fn line(question: &str) -> Result<String> {
  ask(question, false)
}

/// One line from the terminal, not shown while typed.
pub fn secret(question: &str) -> Result<String> {
  ask(question, true)
}

fn ask(question: &str, hidden: bool) -> Result<String> {
  let stdin = std::io::stdin();
  if !stdin.is_terminal() {
    return Err(Error::input(format!(
      "`{}` must be answered on a terminal",
      question.trim_end_matches([':', ' '])
    )));
  }
  eprint!("{question}");
  std::io::stderr().flush()?;
  let _echo = hidden.then(EchoOff::new).flatten();
  let mut answer = String::new();
  stdin.lock().read_line(&mut answer)?;
  Ok(answer.trim().to_owned())
}

/// Terminal echo switched off until dropped (the newline still shows).
#[cfg(unix)]
struct EchoOff(libc::termios);

#[cfg(windows)]
struct EchoOff(windows_sys::Win32::Foundation::HANDLE, u32);

#[cfg(not(any(unix, windows)))]
struct EchoOff;

#[cfg(not(any(unix, windows)))]
impl EchoOff {
  fn new() -> Option<Self> {
    None
  }
}

#[cfg(windows)]
impl EchoOff {
  fn new() -> Option<Self> {
    use windows_sys::Win32::System::Console::{
      ENABLE_ECHO_INPUT, GetConsoleMode, GetStdHandle, STD_INPUT_HANDLE, SetConsoleMode,
    };
    // SAFETY: console mode calls on this process's stdin handle.
    unsafe {
      let handle = GetStdHandle(STD_INPUT_HANDLE);
      let mut mode = 0;
      if GetConsoleMode(handle, &mut mode) == 0 {
        return None;
      }
      (SetConsoleMode(handle, mode & !ENABLE_ECHO_INPUT) != 0).then_some(Self(handle, mode))
    }
  }
}

#[cfg(windows)]
impl Drop for EchoOff {
  fn drop(&mut self) {
    // SAFETY: restores the mode read in `new`.
    unsafe {
      windows_sys::Win32::System::Console::SetConsoleMode(self.0, self.1);
    }
    // Without echo the Enter key left no line break.
    eprintln!();
  }
}

#[cfg(unix)]
impl EchoOff {
  fn new() -> Option<Self> {
    // SAFETY: plain termios calls on stdin with a zeroed struct as output.
    unsafe {
      let mut term: libc::termios = std::mem::zeroed();
      if libc::tcgetattr(libc::STDIN_FILENO, &mut term) != 0 {
        return None;
      }
      let saved = term;
      term.c_lflag &= !libc::ECHO;
      term.c_lflag |= libc::ECHONL;
      (libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &term) == 0).then_some(Self(saved))
    }
  }
}

#[cfg(unix)]
impl Drop for EchoOff {
  fn drop(&mut self) {
    // SAFETY: restores the attributes read in `new`.
    unsafe {
      libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.0);
    }
  }
}
