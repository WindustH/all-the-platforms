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
| `GUIDE.md` | the platform's part of `atp guide` |

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

## Size

- Functions stay under 60 lines. A dispatcher (`match` over commands)
  delegates every arm longer than a few lines to its own function.
- Files stay under about 400 lines; split them by subject.
- Nesting stays within three levels; prefer early returns and `let … else`.

## Shared code in atp-core

Use these instead of writing another copy:

| Need | atp-core |
| --- | --- |
| environment variables, `login --token` fields, saved credentials | `credentials::{env, env_any, fields, expect, saved, save}` |
| access tokens that expire | `token::AccessToken` |
| files to upload, MIME types, image sizes | `file::{Upload, Image, mime_for, sniff}`, `Upload::size` |
| `CHAT/MESSAGE` ids | `chat::{message_id, split_message_id, expect_message_id}` |
| `listen` by polling (fixed or random interval), reconnect delays | `live::{poll, poll_between, Seen, Backoff}` |
| websockets (default User-Agent included; `next_json_lenient` skips non-JSON frames) | `socket::Socket` via `Http::websocket` |
| proxy for non-HTTP connections | `Http::proxy` |
| JSON access | `json::ValueExt` |
| media kind of a MIME type | `MediaKind::from_mime` |
| prices of products, listings, deals, tickets | `Post.price` (`Price::cny`, `original`, `label`); never in `extra` |
| times, counts, text | `text::{from_secs, from_millis, from_beijing, parse_count, html_to_text, encode_uri_component, emoji_shortcode …}` |
| listings | `cli::PageArgs`, `paging` (`paging::slice` for lists held in memory), `PageReq` |
| text and file arguments, confirmations | `cli::{read_text, check_images, check_files, confirm}` |
| HTTP errors | `http::status_error`, `Req::retries` |

Session cookies go only to the hosts in `PlatformInfo::cookie_domains` (and
their subdomains), so list every domain the session is used on.

When a crate needs something a second crate already has, move it into
atp-core first.

## Checks

`cargo fmt`, `cargo build --release` and `cargo clippy --release
--all-targets` stay clean. A refactor leaves the command line unchanged: the
help of every command and every guide topic must read the same before and
after.
