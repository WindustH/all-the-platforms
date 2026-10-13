# Options, environment and files

## Global options

These work anywhere on the command line:

| Option | Environment | Meaning |
| --- | --- | --- |
| `-f, --format` | `SOCIAL_OUTPUT` | `table`, `json`, `yaml`, `jsonl`, `csv` |
| `--json`, `--yaml` | | shortcuts, win over `--format` |
| `--raw` | | include untouched upstream payloads |
| `--proxy URL` | `SOCIAL_PROXY` | http, https or socks5 proxy (`HTTPS_PROXY` / `ALL_PROXY` work too) |
| `--timeout SECS` | | per request, default 30 |
| `--interval SECS` | `SOCIAL_INTERVAL` | minimum gap between requests |
| `-v, --verbose` | `SOCIAL_LOG` | log requests (method, host, status, time) to stderr |

`SOCIAL_LOG` takes a tracing filter, e.g. `SOCIAL_LOG=media=trace` also logs
the names (never the values) of cookies responses set.

## Credentials in the environment

| Variable | Use |
| --- | --- |
| `SOCIAL_<PLATFORM>_COOKIE` | a cookie header for one run, not saved (`SOCIAL_BILI_COOKIE`, `SOCIAL_TWITTER_COOKIE` ...) |
| `REDDIT_CLIENT_ID`, `REDDIT_CLIENT_SECRET`, `REDDIT_USERNAME`, `REDDIT_PASSWORD` | Reddit OAuth "script" app |
| `YOUTUBE_CLIENT_ID`, `YOUTUBE_CLIENT_SECRET`, `YOUTUBE_REFRESH_TOKEN` | YouTube Analytics API |

## Files

| Path | Content |
| --- | --- |
| `~/.config/social-cli/<platform>/session.json` | saved session (mode 0600) |
| `~/.cache/social-cli/<platform>/` | caches (signing keys, query ids, tokens; mode 0600), `#N` lists, the login QR code |

`SOCIAL_CLI_HOME=<dir>` puts both under `<dir>/config` and `<dir>/cache`,
handy for separate accounts:

    SOCIAL_CLI_HOME=~/media-work social bili login

## Shorter commands

Link the binary under a platform name to skip the platform word:

    ln -s "$(which social)" ~/.local/bin/bili
    bili hot -n 5                       # same as: social bili hot -n 5

## Exit codes

0 success; 1 failure (and `status` when not logged in); 2 invalid input;
3 not logged in.
