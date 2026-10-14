# Code conventions

How code in all-the-platforms is laid out and named. The goal: modules that know as
little as possible about each other, small functions, and one shared
implementation of everything more than one platform needs.

## Layers

```
src/main.rs                 registers platforms
crates/atp-<platform>    one platform; depends on atp-core only
crates/atp-meta          Graph API pieces of atp-facebook and atp-instagram
crates/atp-core          everything shared; knows no platform
```

Inside a platform crate, calls go one way:

```
lib.rs (Platform impl) → feature modules → api.rs (transport) → atp-core
                              ↘ parse, refs (pure functions)
```

Feature modules do not call each other's private helpers. Shared pieces of
a crate live in `api`, `parse` or `refs`; shared pieces of several crates
live in atp-core.

## Crate layout

Every platform crate uses the same module names for the same roles:

| Module | Role |
| --- | --- |
| `lib.rs` | `PlatformInfo` and the `Platform` impl: one-line delegations, no logic |
| `api.rs` | the transport type `Api`, which owns the `Ctx`: base URLs, request helpers, signing hook-up, response checks and error mapping |
| `refs.rs` | user input → references: links, ids, `@handles`, `#N` results |
| `parse.rs` / `parse/` | upstream payloads → core model; no I/O |
| `sign.rs` | request signatures |
| `login.rs` | QR and token logins |
| `live.rs` | `listen` (a `gateway.rs` may hold a socket protocol it uses) |
| `extra.rs` / `extra/` | platform-only commands: `pub enum Command`, `pub async fn run(api: &Api, command: Command) -> Result<Data>` |
| feature modules | plural nouns for what they handle: `posts`, `comments`, `users`, `chats`, `messages`, `notifications`, `insights`, `uploads`, `library`, `search` ... |
| `GUIDE.md` | the platform's part of `atp guide` (see [Documentation](#documentation)) |

One name per role: `parse` (not `convert`), `users` (not `people` / `user`),
`messages` (not `im`, `dm`, `pm`, `direct`), `live` (not `stream`,
`listen`, `events`, `sync`).

A platform has exactly one `Api`. Other routes it talks over (a bot API,
MTProto, a second backend) are types named after the route (`Bot`, `Mt`,
`Backend`, `Cloud`) in modules of the same name, held by `Api` or by the
platform struct.

## Names

- The transport parameter is `api: &Api` everywhere; `ctx: &Ctx` only where
  a function needs nothing but the context. Never `c`, `client` or `cl`.
- Actions are verbs (`send`, `delete`, `follow`, `upload`); reads are nouns
  (`user`, `posts`, `comments`). No `get_` / `fetch_` prefixes.
- Parsers are named after what they make: `parse::post`, `parse::user`,
  `parse::comment`, `parse::chat`, `parse::message`; they return `Option`
  when a payload may not be one.
- Parsed references are `*Ref` types; platform-only commands are `Command`.
- Variables use whole words: `post`, `user`, `chat`, `page`, `req` (a
  `PageReq`), `resp` (a response); `v` is fine for a JSON value in parsers.
- Constants are `SCREAMING_SNAKE_CASE`; session extra keys are constants in
  the module that owns them.
- In prose (README, guides, promotion) the project is **ATP** first, with
  all-the-platforms as the secondary, spelled-out name; lowercase `atp` only
  for the command itself. The repository is `WindustH/atp`.

## Size

- Functions stay under 60 lines. A dispatcher (`match` over commands)
  delegates every arm longer than a few lines to its own function.
- Files stay under about 400 lines; split them by subject.
- Nesting stays within three levels; prefer early returns and `let … else`.

## Shared code in atp-core

Use these instead of writing another copy:

| Need | atp-core |
| --- | --- |
| environment variables, `login --token` fields, saved credentials, keys shown masked | `credentials::{env, env_any, fields, expect, saved, save, mask}` |
| access tokens that expire | `token::AccessToken` |
| logins that come back through the browser to a local address | `oauth::{open_consent, redirect}` |
| files to upload, MIME types, image sizes | `file::{Upload, Image, mime_for, sniff}`, `Upload::size` |
| `CHAT/MESSAGE` ids | `chat::{message_id, split_message_id, expect_message_id}` |
| `listen` by polling (fixed or random interval), reconnect delays | `live::{poll, poll_between, Seen, Backoff}` |
| websockets (default User-Agent included; `next_json_lenient` skips non-JSON frames) | `socket::Socket` via `Http::websocket` |
| proxy for non-HTTP connections | `Http::proxy` |
| JSON access | `json::ValueExt` |
| media kind of a MIME type | `MediaKind::from_mime` |
| prices of products, listings, deals, tickets | `Post.price` (`Price::cny`, `original`, `label`); never in `extra` |
| map platforms: points in their system, places, routes, distances, static map sizes | `geo::{Coord, System, Place, Routes, Route, Step, Distance, Polyline, image_size, save_map}`; a place is `Post.place`, never `extra`; `Platform::COORD` for `--coord` |
| times, counts, text | `text::{from_secs, from_millis, from_beijing, parse_count, html_to_text, encode_uri_component, decode_uri_component, emoji_shortcode …}` |
| dates without a time of day (`Oct 12, 2021`, `3 weeks ago`) | `text::date_only`: shown as a date, never with a made-up clock time |
| listings | `cli::PageArgs`, `paging` (`paging::slice` for lists held in memory), `PageReq` |
| lyrics and track lists of music platforms | `music::{Lyrics, with_tracks}` |
| RSS / Atom / JSON feeds: fetching (conditional requests, feed discovery), parsing into posts, `FEED_URL#ENTRY_ID` references | `feed::{load, get, parse, discover, entry_ref, split_entry_ref}` |
| lists and settings a user keeps (neither credentials nor cache) | `Store::{data, put_data}` |
| text and file arguments, confirmations | `cli::{read_text, check_images, check_files, confirm}` |
| HTTP errors | `http::status_error`, `Req::retries` |
| a declared client name for open APIs that challenge look-alike browsers | `http::TOOL_USER_AGENT` |

Session cookies go only to the hosts in `PlatformInfo::cookie_domains` (and
their subdomains), so list every domain the session is used on.

When a crate needs something a second crate already has, move it into
atp-core first.

## Documentation

`atp guide`, the MCP `help` tool and the `atp://guide/…` resources are
built from four sources. A platform is not done until all four are
complete.

- **clap doc comments.** Every command, nested subcommand and argument of
  a platform (`extra.rs`), positionals included, has a `///` comment;
  `-y, --yes` reads "Skip the confirmation question (needed in scripts)".
  Commands whose usage is not obvious (several argument forms, special
  syntax, nested subcommands, a particular login) get examples in
  `after_long_help`: a `const EX_…` in the `Examples:` format of
  atp-core, ending with `More: atp guide <id> <command>`.
- **`crates/atp-core/guide/commands/*.md`.** What a shared command means
  on every platform: arguments, paging, `--since` / `--until`, `#N`,
  confirmation and `--yes`, output shapes. `guide/command.rs` maps command
  names to files; a new shared command gets a file or joins one.
- **The crate's `GUIDE.md`**, in this order: an optional intro,
  `## Logging in`, `## References`, `## Commands`, other sections,
  `## Limits`.
  - `## Logging in` is required and complete: every way in with its
    steps, where tokens and app credentials come from, what each way
    unlocks and what works without an account, how long a session lasts
    and how its end shows, risk control (new-device checks, captchas,
    write pacing, ban risks) and the environment variables. Facts the
    code cannot tell are said to be unknown, never guessed.
  - Under `## Commands`, a command that behaves specially on the platform
    gets a `### name` section (`### playlist create`; one section may name
    several: `### like, unlike`). Headings hold only command names as
    typed, canonical names for shared commands (`read`, not `show`).
    Commands with nothing special get no section. `atp guide <id>
    <command>` shows the sections naming the command or its group; `login`
    shows `## Logging in`.
- **`PlatformInfo`**: `kind`, `token` (what `login --token` takes, as a
  placeholder), `env` (credential variables) and `guest` (what works
  without logging in) feed `atp guide login`, `atp platforms` and login
  hints.

## Checks

`cargo fmt`, `cargo build --release` and `cargo clippy --release
--all-targets` stay clean. A refactor leaves the command line unchanged: the
help of every command and every guide topic must read the same before and
after.
