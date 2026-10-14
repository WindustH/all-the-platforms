## Logging in

Every command talks to one Mattermost server with a token; nothing works
without logging in.

### Personal access token

`atp mattermost login --token 'https://chat.example.com|TOKEN'` with a
personal access token (Profile → Security → Personal Access Tokens; the
server admin may need to enable them). A server without `https://` gets it
added.

### Password

`atp mattermost login --token 'https://chat.example.com|LOGIN|PASSWORD'`
logs in with the password and saves the session token it yields. MFA and
SSO accounts need a personal access token.

The parts may also be separated by spaces
(`'https://chat.example.com TOKEN'`); use `|` when the password has spaces.
`-` reads the value from stdin. Either way the account is checked before
it is saved.

### When the token stops working

A personal access token works until it is revoked; a password login's
session lasts as long as the server keeps it. Then commands fail with
`not_authenticated`; log in again.

### Environment variables

- `MATTERMOST_URL` and `MATTERMOST_TOKEN`: the server and a token for one
  run, both needed; never saved, and they win over the saved session.

## References

- Posts: permalinks (`https://SERVER/TEAM/pl/ID`), post ids, or
  `CHANNEL/POST`. `#N` works after a list.
- Channels: ids, links (`https://SERVER/TEAM/channels/NAME`, or
  `…/messages/@user` for a direct channel), `TEAM/NAME`, `~name` / `name`
  (looked up in your teams), or `@user` for the direct channel. Teams: ids
  or names.
- Users: ids, `@name`, emails, `me`.

## Commands

### chats

`chats` lists your teams and direct / group channels; `chats TEAM` the
team's channels you belong to (most recent first).

### messages

`messages CHANNEL [-n N] [--since 7d]`.

### send

`send CHANNEL "text" [-a FILE …] [--reply-to POST]`: the reply goes into
the post's thread.

### comment, comments

`comment POST "text"` replies in the post's thread; `comments POST` lists
the thread.

### react

`react POST 👍`, or a name like `tada`; `--undo` removes it.

### members

`members CHANNEL` or `members TEAM`.

### listen

`listen [CHANNEL …]` streams new, edited and deleted posts and reactions
over the server's websocket; dropped connections are reopened.

### search

`search 'terms from:ana in:town-square after:2026-01-01'` searches your
teams; `team:NAME` narrows it to one.

### notifications, unread

`notifications` lists posts that mention you; `unread` counts per team.

### favorites, favorite, unfavorite

`favorites` lists your flagged posts; `favorite POST [--undo]` flags one.

### download

`download POST` saves its files.

### teams, channel

`teams` lists your teams; `channel CHANNEL` shows a channel's purpose,
header and message count.

### pins, pin

`pins CHANNEL` lists pinned posts; `pin POST [--undo]` pins or unpins.

### join, leave

`join CHANNEL`, `leave CHANNEL`.

## Limits

Requests are spaced at least 0.2 s apart and writes wait 0.2–0.6 s first.
When the server rate-limits (429), ATP waits until its reset (up to 30 s,
a few times) before it fails with `rate_limited`. A server that is older or
has a feature turned off answers `unsupported_operation`.
