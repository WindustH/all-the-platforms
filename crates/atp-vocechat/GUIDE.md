VoceChat is self-hosted: every command talks to the server you logged in
to.

## Logging in

`SERVER` below is the server's address; without a scheme, `https://` is
assumed (`chat.example.com`). Separate the parts with `|`.

### As a user

    atp vocechat login --token 'SERVER|EMAIL|PASSWORD'

logs in with the account's email and password (`--token -` reads the value
from stdin, which keeps the password out of the shell history). The server
returns an access token and a refresh token; ATP keeps both and never the
password. A user can do everything below.

The access token expires within minutes and is renewed with the refresh
token when the server answers 401 (once per run). How long the refresh
token lasts is not known; when renewal fails, commands fail with
`not_authenticated` ("the VoceChat session expired and could not be
renewed"): log in again.

### As a bot

    atp vocechat login --token 'SERVER|BOT_API_KEY'

uses a bot's API key (Settings → Bot & Webhook); it is checked against the
server before it is kept. Bots can send to users and to the channels they
are in (without `--reply-to`), list those channels, and look up users and
members. No history (`messages`, `read`, `download`), edits, deletions,
reactions or live events: VoceChat delivers those to bots by webhook, and
these commands fail with `unsupported_operation`.

When the server refuses a key or token, commands fail with
`not_authenticated` ("VoceChat refused the credentials").

### Environment

`VOCECHAT_URL` + `VOCECHAT_API_KEY` (both) supply a bot for one run; they
win over the saved session and are never written.

## References

- Chats: `group:<gid>` (a channel), `user:<uid>` (a direct chat), or the web
  app's `.../#/chat/channel/<gid>` / `#/chat/dm/<uid>` links. `#N` works
  after `chats`.
- Messages: `group:<gid>/<mid>` / `user:<uid>/<mid>` (the `id` of listed
  messages), or `#N` after `messages`.
- Users: `user:<uid>`, the number, or `me`.

## Commands

### chats

Your channels (a bot's: the channels it is in), then a direct chat with
every other user of the server (users only: a bot key cannot list users).
Channels have nothing inside them.

### messages

Users only. Up to 100 messages per page; edits, deletions and reactions are
folded into the messages they touch.

### read

Users only.

### members

A channel's members (every user, for a public channel without a member
list), or both people of a direct chat.

### user

`user:<uid>`, the number, or `me`; works for bots too.

### send

Text goes as plain text; each file is uploaded and sent as its own message.
`--reply-to` replies with the first part (users only).

### markdown

Like `send`, but the text is Markdown.

### edit, delete

Users only.

### react

Users only. VoceChat takes ❤️ 😄 👀 👍 👎 🎉 🙁 🚀. Sending the same reaction
again takes it back, so `--undo` sends that same request.

### download

Users only: the image, video or audio of a message.

### listen

Users only. Follows the user event stream; on a reconnect it continues
after the last message seen. When the server removes this client (the
account logged in elsewhere or was removed) `listen` stops with
`not_authenticated`; after five failed connections in a row it gives up.

## Limits

No search (VoceChat has no search API) and no pinning yet. Requests are
spaced at least 200 ms apart, and sends wait a short random pause
(0.2 to 0.6 seconds) first.
