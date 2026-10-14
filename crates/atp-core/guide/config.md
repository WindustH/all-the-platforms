# Options, environment and files

## Global options

These work anywhere on the command line:

| Option | Environment | Meaning |
| --- | --- | --- |
| `-f, --format` | `ATP_OUTPUT` | `table`, `json`, `yaml`, `jsonl`, `csv` |
| `--json`, `--yaml` | | shortcuts, win over `--format` |
| `--raw` | | include untouched upstream payloads |
| `--proxy URL` | `ATP_PROXY` | http, https or socks5 proxy (`HTTPS_PROXY` / `ALL_PROXY` work too) |
| `--timeout SECS` | | per request, default 30 |
| `--interval SECS` | `ATP_INTERVAL` | minimum gap between requests |
| `-v, --verbose` | `ATP_LOG` | log requests (method, host, status, time) to stderr |
| `--no-verify` | `ATP_NO_VERIFY=1` | fail on captchas instead of opening them in a browser |
| | `ATP_BROWSER` | the browser for human checks: a path or a command (`chromium`) |

Yes / no variables such as `ATP_NO_VERIFY` take `1` / `0`, `true` /
`false`, `yes` / `no` or `on` / `off`.

`ATP_LOG` takes a tracing filter, e.g. `ATP_LOG=media=trace` also logs
the names (never the values) of cookies responses set.

## Credentials in the environment

| Variable | Use |
| --- | --- |
| `ATP_<PLATFORM>_COOKIE` | a cookie header for one run, not saved (`ATP_BILI_COOKIE`, `ATP_TWITTER_COOKIE` ...) |
| `REDDIT_CLIENT_ID`, `REDDIT_CLIENT_SECRET`, `REDDIT_USERNAME`, `REDDIT_PASSWORD` | Reddit OAuth "script" app |
| `YOUTUBE_CLIENT_ID`, `YOUTUBE_CLIENT_SECRET`, `YOUTUBE_REFRESH_TOKEN` | YouTube Analytics API |

## Files

| Path | Content |
| --- | --- |
| `<config>/all-the-platforms/<platform>/session.json` | saved session (only you can read it) |
| `<cache>/all-the-platforms/<platform>/` | caches (signing keys, query ids, tokens), `#N` lists, the login QR code |

| System | `<config>` | `<cache>` |
| --- | --- | --- |
| Linux | `~/.config` (`$XDG_CONFIG_HOME`) | `~/.cache` (`$XDG_CACHE_HOME`) |
| macOS | `~/Library/Application Support` | `~/Library/Caches` |
| Windows | `%APPDATA%` | `%LOCALAPPDATA%` |

`ATP_HOME=<dir>` puts both under `<dir>/config` and `<dir>/cache`,
handy for separate accounts:

    ATP_HOME=~/media-work atp bili login

## Shorter commands

Link the binary under a platform name to skip the platform word:

    ln -s "$(which atp)" ~/.local/bin/bili
    bili hot -n 5                       # same as: atp bili hot -n 5

On Windows, a copy or a hard link named after the platform does the same:
`mklink /H bili.exe atp.exe`.

## Exit codes

0 success; 1 failure (and `status` when not logged in); 2 invalid input;
3 not logged in.
