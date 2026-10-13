# Architecture

social-cli is one binary (`social`) that drives several social platforms through
the same command set. The code is split so that each part stays small and
knows as little as possible about the others.

```
src/main.rs                 registry: lists the platforms, nothing else
crates/social-core          platform-agnostic kernel
crates/social-<platform>    one crate per platform (social-zhihu, social-telegram ...)
crates/social-meta          Graph API pieces shared by social-facebook and social-instagram
```

Naming, layout and size rules are in [conventions.md](conventions.md).

Dependencies only point downwards: the binary depends on every crate,
platform crates depend on `social-core` only, and `social-core` knows no
platform. Adding a platform means adding a crate and one `.platform::<P>()`
line in `src/main.rs`.

## social-core

| Module | Role |
| --- | --- |
| `platform` | `Platform` trait; `cap` (capabilities), `info` (`PlatformInfo`, `Choices`, login ways), `request` (`PageReq`, `Query`, `Draft`, `Reply`, `Message`, `QrTicket`) |
| `ctx` | `Ctx`: what a platform works with (HTTP client with the session cookies, files, session extras, login hints, `#N` resolution) |
| `model` | normalized `Post`, `User`, `Comment`, `Collection`, `Notification`, `Page<T>`, `Action`, `AuthStatus`, `Insights`, `Transcript`, and `Data` (everything a command can print), one file per type |
| `cli` | the shared commands, one module per area (`account`, `read`, `library`, `write`, `media`, `chat`) flattened into `CommonCommand`; per-platform clap command; global and paging options |
| `app` | top-level program, `social platforms`, multi-call (`bili` symlink) |
| `account` | `login` (QR / token / browser / cookie string), `logout`, `status` |
| `http` | Chrome-fingerprinted client (wreq), cookie jar, pacing, proxy; `request` (building, sending, retries, streaming), `response` |
| `store` | session file (0600), TTL cache, `#N` short-index lists |
| `paging` | `collect`: follow cursors until `--limit` items; `collect_window`: `--since` / `--until` |
| `download` | save media, merge DASH audio/video, audio-only, WAV segments (ffmpeg) |
| `file` | format detection by magic bytes, MIME types, files and images to upload |
| `guide` | `social guide`: topics from `crates/social-core/guide/*.md`, platform topics generated from the command set plus each crate's `GUIDE.md` |
| `output` | human tables/cards, the `{ok, schema_version, platform, fetched_at, data}` envelope, and flat JSON Lines / CSV rows |
| `json` | `ValueExt`: dotted-path getters tolerant of upstream quirks (`v.str("a.b.0")`, `v.count("stat.like")`) |
| `text` | counts (`1.2万`), durations, truncation, HTML to text, timestamps |
| `browser` | import cookies from local browsers (`browser` feature, on by default) |
| `credentials` | environment variables, `login --token` fields, credentials saved in the session |
| `token` | `AccessToken`: expiring access tokens cached in memory and in the private cache |
| `chat` | `CHAT/MESSAGE` ids of messages without permalinks |
| `live` | `listen` helpers: polling with de-duplication, reconnect back-off |
| `socket` | websocket connections through the same client |
| `qr` | login QR codes in the terminal, from a URL or a QR image |

### The Platform contract

A platform crate exports one type implementing `social_core::Platform`:

- `const INFO: PlatformInfo` declares id, aliases, home page, cookie domains,
  required cookies, supported capabilities (`caps`), accepted option values
  (`choices`) and request pacing (`min_interval`).
- Methods for the shared commands (`search`, `read`, `comments`, `like`,
  `publish`, ...). Each has a default body returning `unsupported_operation`,
  so a platform implements only what it supports and lists it in `caps`;
  unsupported commands are hidden from its `--help`.
- Listing methods return one `Page<T>` per call for a `PageReq { cursor, size }`;
  the core follows `next_cursor` until `--limit` is reached. Cursors are opaque
  strings owned by the platform (offset, page number, upstream cursor ...).
- Post / user arguments arrive as strings with `#N` already resolved; the
  platform accepts its own ids and URLs (at least the `url` it puts in
  `Post.url` / `User.url`, because that is what `#N` resolves to).
- `type Extra: clap::Subcommand` holds platform-only commands; `run_extra`
  returns `Data` like everything else.

### Chat platforms

Messaging platforms use the same model: a chat (server, channel, group,
direct conversation, thread) is a `Collection` and a message is a `Post`. The
chat commands (`chats [PARENT]`, `messages CHAT`, `send CHAT`, `members`,
`react`, `edit`, `listen`) map to trait methods behind `Cap::Chats`,
`Messages`, `Send`, `Members`, `React`, `Edit` and `Listen`, and stay hidden
on platforms without them. `delete`, `read`, `search`, `user` and `download`
work on messages as on posts. A message without a permalink takes
`CHAT/MESSAGE` as its `id`, so it can be passed back like a link; `#N`
resolves chats and messages from the last list. `listen` hands each event to
an `output::Stream`, which prints it at once (a line, or one JSON object /
YAML document per event).

Credentials that are not cookies (bot tokens, API keys, `AppID:Secret`) come
in through `login --token` (`Cap::TokenLogin`, `Platform::login_token`) and
are kept in the session extras, or from environment variables.

### Flow of one command

1. `app` parses arguments, picks the platform, starts a current-thread tokio runtime.
2. `cli::execute` loads the session (`SOCIAL_<ID>_COOKIE` wins over the saved
   file), builds `Http` with those cookies and calls `P::new(ctx)`.
3. The shared command maps to a trait method (listings go through `paging::collect`).
4. Cookies changed by responses are written back to the session file (logged-in sessions only).
5. Lists are remembered for `#N`; `raw` payloads are dropped unless `--raw`;
   `output` prints a table or the JSON/YAML envelope.

## Inside a platform crate

Every platform follows the same layout so each file has one job:

| File | Role |
| --- | --- |
| `lib.rs` | `PlatformInfo` and the `Platform` impl: thin, delegates to the modules below |
| `api.rs` | the platform's transport: base headers, signing hook, response envelope → `Error` |
| `sign.rs` (or `sign/`) | request signatures (WBI, x-s, x-client-transaction-id, SAPISIDHASH ...) |
| `refs.rs` | parse ids and URLs into typed references |
| `parse.rs` | upstream JSON → core models |
| domain modules (`read.rs`, `write.rs`, `account.rs` ...) | endpoints grouped by domain |
| `extra.rs` | the `Extra` subcommand enum and its handlers |

Rules of thumb:

- Parse upstream JSON with `ValueExt` instead of mirror structs; keep the
  untouched object in the model's `raw` field.
- Map upstream failures to the shared `ErrorCode`s (login → `not_authenticated`,
  captcha → `verification_required`, ...), so scripts can react the same way everywhere.
- Long-lived upstream metadata (signing keys, GraphQL query ids) goes into
  `ctx.store.cache_get/cache_put` with a TTL.
- Write operations pause briefly (`ctx.http.pause`) where the platform is known
  to flag bursts.
