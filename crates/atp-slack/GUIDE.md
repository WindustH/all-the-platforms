## Logging in

Slack is reached through a Slack app of your own (https://api.slack.com/apps
→ Create New App → install it to the workspace) and its tokens. Nothing
works without a token.

### Bot or user token

- `atp slack login --token xoxb-…` with the app's **bot** token, or
  `--token xoxp-…` with its **user** token (acts as you; needed for
  `search` and `notifications`). `-` reads the token from stdin.
- The token is checked with `auth.test` before it is saved. Browser tokens
  (`xoxc-`) are refused.
- A bot reads and posts only where it is a member: invite the app to the
  channel (`/invite @app`), or `atp slack join CHANNEL` for public ones.
  Some methods need the other kind of token; the error says so.

### Live events (Socket Mode)

For live events add the app-level token (Basic Information → App-Level
Tokens, scope `connections:write`) and enable Socket Mode and the event
subscriptions (`message.channels`, `message.groups`, `message.im`,
`message.mpim`, `reaction_added`, `reaction_removed`):
`atp slack login --token 'xoxb-… xapp-…'` (a space or `|` between them).
An app-level token alone cannot read anything.

### Scopes

Useful scopes: `channels:history`, `groups:history`, `im:history`,
`mpim:history`, `channels:read`, `groups:read`, `im:read`, `mpim:read`,
`chat:write`, `files:read`, `files:write`, `reactions:read`,
`reactions:write`, `pins:read`, `pins:write`, `users:read`,
`users:read.email`, `bookmarks:read`, `team:read`, `channels:join`,
`im:write`, and for user tokens `search:read`. A missing scope is named in
the error: add it to the app, reinstall the app and log in with the new
token.

### When the token stops working

How long a token lasts is up to Slack. When it is revoked or expires, or
the app is uninstalled, commands fail with `not_authenticated`;
log in again with a fresh token.

### Environment variables

- `SLACK_TOKEN` (or `SLACK_USER_TOKEN` / `SLACK_BOT_TOKEN`, the first one
  set wins): a bot or user token for one run, never saved; it wins over the
  saved token.
- `SLACK_APP_TOKEN`: the app-level `xapp-` token for `listen`.

## References

- Messages: links (`https://acme.slack.com/archives/C…/p1700000000000100`,
  thread replies with `?thread_ts=…`) or `CHANNEL/TS`, the `id` of listed
  messages. `#N` works after a list.
- Conversations: ids (`C…`, `G…`, `D…`), archive links, `#name` / `name`,
  or a user (`@name`, `U…`, `<@U…>`) for your direct conversation.
- Users: `U…` / `W…` ids, `<@U…>`, `@name` / `name`, an email, or `me`.

## Commands

### chats

`chats` lists the conversations the token's account is in.

### messages

`messages CHAT [-n N] [--since 7d]` pages back through history.

### send

`send CHAT "text" [-a FILE …] [--reply-to MESSAGE]`: a reply goes into the
message's thread and must be in the same conversation.

### comment, comments

`comment MESSAGE "text"` replies in its thread and `comments MESSAGE`
lists the thread.

### react

`react MESSAGE 👍`, or a name like `tada`; `--undo` removes it.

### search

`search QUERY` takes Slack's own syntax
(`in:#general from:@ana has:link after:2026-01-01`); `--sort newest`.
Needs a user token.
`search -t user NAME` finds workspace members whose name, display name or
email contains NAME, with either token.

### notifications

`notifications` lists messages that mention you. Needs a user token.

### listen

`listen [CHAT …]` is live with Socket Mode: messages, edits, deletions and
reactions. Without an app-level token it polls the named conversations
every ten seconds, so it needs at least one.

### download

`download MESSAGE` saves its files.

### workspace

`workspace` shows the workspace's name, domain and address.

### pins, pin

`pins CHAT` lists pinned messages; `pin MESSAGE [--undo]` pins or unpins.

### bookmarks

`bookmarks CHAT` lists the links bookmarked in a conversation.

### join, leave

`join CHANNEL` joins a public channel; `leave CHAT` leaves a conversation.

## Limits

When Slack rate-limits a method, ATP waits as long as Slack says (up to a
minute, a few times) before it fails with `rate_limited`. Requests are
spaced at least 0.3 s apart, and user-token writes wait 0.3–0.9 s first.
