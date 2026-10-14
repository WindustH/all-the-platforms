# Architecture

ATP (all-the-platforms) is one binary (`atp`) that drives social, video and chat platforms through
the same command set. The code is split so that each part stays small and
knows as little as possible about the others.

```
src/main.rs                 registry: lists the platforms, nothing else
crates/atp-core          platform-agnostic kernel
crates/atp-<platform>    one crate per platform (atp-zhihu, atp-telegram ...)
crates/atp-meta          Graph API pieces shared by atp-facebook and atp-instagram
```

Naming, layout and size rules are in [conventions.md](conventions.md).

Dependencies only point downwards: the binary depends on every crate,
platform crates depend on `atp-core` only, and `atp-core` knows no
platform. Adding a platform means adding a crate and one `.platform::<P>()`
line in `src/main.rs`.

## atp-core

| Module | Role |
| --- | --- |
| `platform` | `Platform` trait; `cap` (capabilities), `info` (`PlatformInfo`, `Choices`, login ways), `request` (`PageReq`, `Query`, `Draft`, `Reply`, `Message`, `QrTicket`) |
| `ctx` | `Ctx`: what a platform works with (HTTP client with the session cookies, files, session extras, login hints, `#N` resolution) |
| `model` | normalized `Post`, `User`, `Comment`, `Collection`, `Notification`, `FileEntry` (drives), `Page<T>`, `Action`, `AuthStatus`, `Insights`, `Transcript`, and `Data` (everything a command can print), one file per type |
| `cli` | the shared commands, one module per area (`account`, `read`, `library`, `write`, `media`, `chat`) flattened into `CommonCommand`; per-platform clap command; global and paging options |
| `app` | top-level program, multi-call (`bili` symlink) |
| `platforms` | `atp platforms`: one row per platform grouped by kind, `atp platforms <id>` in detail, the same data as JSON |
| `account` | `login` (QR / token / browser / cookie string), `logout`, `status` |
| `http` | Chrome-fingerprinted client (wreq), cookie jar, pacing, proxy; `request` (building, sending, retries, streaming), `response` |
| `store` | session file (0600), data files the user keeps beside it (`data`, `put_data`: lists and settings `logout` leaves alone), TTL cache (values and raw bytes), `#N` short-index lists |
| `paging` | `collect`: follow cursors until `--limit` items; `collect_window`: `--since` / `--until` |
| `download` | save media, merge DASH audio/video, audio-only, WAV segments (ffmpeg); `files`: a drive's files and folders under their own names, through `.part` files resumed by `Range`, with per-link headers and links renewed mid-file |
| `upload` | files too big for memory: `LocalFile`, `Hashes` (md5, sha1, 4 MB block md5s, md5 of the first 256 KB in one read), `Parts` (part by part with retries, a progress bar and resume state in the private cache; optionally the SHA-1 state before each part, `Sha1Midway`) |
| `task` | background work upstream: `wait` polls a task with growing gaps; `retry` backs off while a call fails with errors a predicate picks (rate limits, risk codes) |
| `oauth` | OAuth device-code logins (`DeviceCode`, `Poll`, `device_login`), the consent page of logins that come back through the browser (`open_consent`) and the one-shot loopback listener they come back to, with the parameters in the query or after `#` (`redirect`), and `Tokens` kept in the session extras, saved to disk the moment a single-use refresh token rotates (`access_token`) |
| `file` | format detection by magic bytes, MIME types, files and images to upload |
| `geo` | map platforms: `Coord` (a point in its `System`: `wgs84`, `gcj02`, `bd09`) and the conversions between them, mainland China's outline for Google, `Place` (`Post.place`), `Routes` / `Route` / `Step`, `Distance`, `Polyline` (Google's encoded format), `localize` (every point of a result into the `--coord` system), static map sizes |
| `guide` | `atp guide`: topics from `crates/atp-core/guide/*.md` and what shared commands mean from `guide/commands/*.md`; platform, command (`atp guide bili search`) and login pages generated from the clap command set and `PlatformInfo`, plus each crate's `GUIDE.md` whole or by section; `page` resolves the words for the CLI and MCP |
| `output` | human tables/cards, the `{ok, schema_version, platform, fetched_at, data}` envelope, and flat JSON Lines / CSV rows |
| `json` | `ValueExt`: dotted-path getters tolerant of upstream quirks (`v.str("a.b.0")`, `v.count("stat.like")`) |
| `text` | counts (`1.2万`), durations, truncation, HTML to text, timestamps |
| `feed` | web feeds for platforms that read them (podcasts, RSS): RSS 0.9x–2.0, Atom and JSON Feed through feed-rs into a `Feed` whose entries are posts (enclosures as media, iTunes episode data in `extra`); `get` fetches as feed readers do (a declared client, `If-None-Match` / `If-Modified-Since` against a private cached copy), `load` also follows a web page to the feed it announces (`discover`); `FEED_URL#ENTRY_ID` entry references (`entry_ref`, `split_entry_ref`) |
| `music` | what music platforms share: `Lyrics` (LRC with its translation, as plain text or LRC) and track lists inside a post (`with_tracks`) |
| `browser` | import cookies from local browsers (`browser` feature, on by default) |
| `credentials` | environment variables, `login --token` fields, credentials saved in the session |
| `token` | `AccessToken`: expiring access tokens cached in memory and in the private cache |
| `chat` | `CHAT/MESSAGE` ids of messages without permalinks |
| `live` | `listen` helpers: polling with de-duplication, `Backoff` (reconnect delays, task polling) |
| `socket` | websocket connections through the same client |
| `qr` | login QR codes in the terminal, from a URL or a QR image |
| `verify` | human checks by hand: a Chromium-family browser over DevTools with the session cookies, then one retry (`chrome`, `cdp`) |
| `mcp` | `atp mcp`: JSON-RPC over stdio; tools `platforms`, `help`, `read`, `run` calling commands without printing; the guide as resources (`atp://guide/…`) |

### The Platform contract

A platform crate exports one type implementing `atp_core::Platform`:

- `const INFO: PlatformInfo` declares id, aliases, home page, cookie domains,
  required cookies, supported capabilities (`caps`), accepted option values
  (`choices`), request pacing (`min_interval`), its kind, what logging in
  takes (`token`, `env`) and allows without an account (`guest`), and the
  crate's `GUIDE.md` (`guide`).
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
- `const COORD: Option<geo::System>`: map platforms name the coordinate
  system they work in; it is the default of the `--coord` option they get.

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

### Drives

Cloud drives (`Cap::Files`) list `FileEntry`s (id, name, path, folder or
file, size, time, md5, share link) where other platforms list posts. The
shared `search`, `read`, `download` and `delete` call `search_files`,
`file`, `files` + `file_link` and `delete` instead of the post methods;
`#N` after a listing refers to an entry's `id`. `download::files` walks a
folder through `files`, saves under the drive's names and asks
`file_link(file, renew)` for a link (with the headers its host wants)
again when the host refuses one mid-file; session cookies go to a link
only when it lists them, and never across sites on redirects. Uploads use
`upload::Parts`, background operations `task::wait`, and drives with an
open platform `oauth`. Everything else (`ls`, `mkdir`, `mv`, shares, the
recycle bin) is the drive's own `Extra` commands.

### Map platforms

Map platforms (`amap`, `gmap`, `bmap`, `qqmap`) share command names and
options: places through `search`, `read` and `download`, and their own
`nearby`, `tips`, `geocode`, `regeo`, `route`, `distance` and `staticmap`
with the same arguments wherever the APIs allow. A place is a `Post` with a
`geo::Place` (point, address, category, phone, rating, distance, region);
`route` returns `Data::Routes` (routes with distance in metres, duration in
seconds, steps and a polyline) and `distance` returns `Data::Distances`.
Tables, cards and jsonl/csv rows know these shapes.

Every coordinate carries its `geo::System`. `Platform::COORD` gives a
platform a global `--coord` option defaulting to it; the CLI puts its value
in `Ctx::coord`. The platform reads user points in that system and converts
them to what its API takes (Amap and Tencent GCJ-02; Baidu BD-09 or, through
`coord_type` / `ret_coordtype`, GCJ-02; Google WGS-84, or GCJ-02 inside
mainland China), and tags the points it parses with the system the API
answered in. After the command, `geo::localize` converts every typed point
(places, routes, distances) and every `{lat, lng, system}` object in values
and `extra` into the `--coord` system.

### Flow of one command

1. `app` parses arguments, picks the platform, starts a current-thread tokio runtime.
2. `cli::execute` loads the session (`ATP_<ID>_COOKIE` wins over the saved
   file), builds `Http` with those cookies and calls `P::new(ctx)`.
3. The shared command maps to a trait method (listings go through `paging::collect`);
   on map platforms the points of the result are converted to the `--coord` system.
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
