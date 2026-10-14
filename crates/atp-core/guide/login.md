# Logging in

A session is a set of cookies (plus, for some platforms, tokens) saved per
platform. Every command uses it automatically.

## Ways to log in

    atp <platform> login              # QR code where supported, else the browser
    atp <platform> login --qrcode     # scan with the platform's mobile app
    atp <platform> login --browser    # read the session from local browsers
    atp <platform> login --browser firefox
    atp <platform> login --cookie 'name=value; name2=value2'

- **QR code** (Zhihu, Xiaohongshu, Bilibili): the code is drawn in the
  terminal and saved as an SVG under the cache directory; scan it with the
  app and confirm. Xiaohongshu may ask the new device for a captcha after the
  scan; use `--browser` there.
- **Browser**: reads the cookies of a browser where you are logged in
  (Chrome, Chromium, Edge, Brave, Firefox, LibreWolf, Zen, Vivaldi, Opera,
  Arc). Only the platform's own domains are read. Log in on the website first.
- **Cookie string**: copy the `Cookie` request header from the browser's
  developer tools (Network tab, any request to the site).

`login` verifies the session with `whoami` before saving it.

## Checking and removing sessions

    atp <platform> status             # exit code 0 when logged in, 1 when not
    atp <platform> whoami             # the account
    atp <platform> logout             # delete the saved session

## Without saving

- `ATP_<PLATFORM>_COOKIE`, with the platform's name rather than an alias
  (`ATP_BILI_COOKIE`, `ATP_TWITTER_COOKIE`), supplies a cookie header for
  one run; it wins over the saved session and is never written.
- Reddit: `REDDIT_CLIENT_ID`, `REDDIT_CLIENT_SECRET`, `REDDIT_USERNAME`,
  `REDDIT_PASSWORD` switch to Reddit's OAuth API (a "script" app).
- YouTube analytics: `YOUTUBE_CLIENT_ID`, `YOUTUBE_CLIENT_SECRET`,
  `YOUTUBE_REFRESH_TOKEN` (see `atp guide youtube`).

## Where sessions live

`~/.config/all-the-platforms/<platform>/session.json` on Linux (macOS:
`~/Library/Application Support/…`, Windows: `%APPDATA%\…`), readable by
you only.
`ATP_HOME=<dir>` moves config and cache under `<dir>`. Only logged-in
sessions are saved; cookies a platform refreshes are written back.

## When a platform pushes back

| Error code | Meaning | What to do |
| --- | --- | --- |
| `not_authenticated` | no session, or it expired | log in again |
| `verification_required` | captcha / risk control | pass the check in the browser (below), wait, or change network (`--proxy`) |
| `rate_limited` | too many requests | wait; slow down with `--interval` |
| `ip_blocked` | the network is blocked | change network |

## Captchas and risk checks

When a platform asks for a human check and atp runs on a terminal, it opens
the check in a browser instead of failing:

1. Chrome, Chromium, Edge or Brave starts on a throwaway profile that
   carries the atp session, through the same proxy as atp.
2. Pass the check there, then press Enter in the terminal.
3. The cookies the site set come back into the session (saved when logged
   in) and the command runs once more.

Without one of those browsers the page opens in the default browser and the
command is retried as is; checks tied to the network clear that way, checks
tied to cookies may need a new `login`. `ATP_BROWSER` picks the browser (a
path or a command name). Pipes, scripts and `--no-verify` (`ATP_NO_VERIFY=1`)
get the error with the page in `error.url` instead. atp never solves
captchas itself.
