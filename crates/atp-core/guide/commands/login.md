Logs in and saves the session for this platform; every later command uses
it. Without options, `login` shows a QR code where the platform has one,
and otherwise reads the session of a local browser; a platform without
cookies (tokens, API keys, OAuth apps) names its ways to log in instead.

    atp bili login                    # QR code
    atp x login --browser firefox
    atp zhihu login --cookie 'z_c0=...; _xsrf=...'
    atp tg login --token -            # the token from standard input

- `--qrcode` draws a code in the terminal (and saves it as an image under
  the cache directory) to scan with the platform's app. ATP waits up to
  three minutes for the confirmation.
- `--browser [NAME]` reads the platform's own cookies from a browser where
  you are logged in: chrome, chromium, edge, brave, firefox, librewolf,
  zen, vivaldi, opera or arc. Without NAME each one is tried and the first
  holding a complete session wins.
- `--cookie 'a=1; b=2'` takes a `Cookie` header copied from the browser's
  developer tools.
- `--token TOKEN` takes an access token or API credentials in the form the
  platform names; `-` reads it from standard input, keeping it out of the
  shell history.
- One way per call; `--qrcode` or `--token` on a platform without them
  answers `unsupported_operation`.
- Before saving, ATP checks that the session has the cookies the platform
  requires and asks the platform who you are (`whoami`). A session that
  fails either check is not saved (`not_authenticated`).
- The result is Auth: `authenticated`, `user`, `source` (`qrcode`,
  `cookie`, `browser:NAME`, `token`), `saved_at`.
- For one run without saving, set `ATP_<ID>_COOKIE` (`ATP_BILI_COOKIE`).
- Over MCP no QR code can be shown and `-` is not available: pass
  `--cookie`, `--token` or `--browser`, or log in once on a terminal.

More: `atp guide login`
