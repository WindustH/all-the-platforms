## Logging in

Slack is reached through a Slack app of your own (https://api.slack.com/apps
→ Create New App → install it to the workspace):

- `social slack login --token xoxb-…` with the app's **bot** token, or
  `--token xoxp-…` with its **user** token (acts as you; needed for search
  and mentions). `-` reads the token from stdin.
- For live events add the app-level token (Basic Information → App-Level
  Tokens, scope `connections:write`) and enable Socket Mode and the event
  subscriptions (`message.channels`, `message.groups`, `message.im`,
  `message.mpim`, `reaction_added`):
  `social slack login --token 'xoxb-… xapp-…'`.
- `SLACK_TOKEN` (or `SLACK_BOT_TOKEN` / `SLACK_USER_TOKEN`) and
  `SLACK_APP_TOKEN` supply tokens for one run without saving them.

Useful scopes: `channels:history`, `groups:history`, `im:history`,
`mpim:history`, `channels:read`, `groups:read`, `im:read`, `mpim:read`,
`chat:write`, `files:read`, `files:write`, `reactions:read`,
`reactions:write`, `pins:read`, `pins:write`, `users:read`,
`users:read.email`, `bookmarks:read`, `team:read`, `channels:join`,
`im:write`, and for user tokens `search:read`. A missing scope is named in
the error.

## References

- Messages: links (`https://acme.slack.com/archives/C…/p1700000000000100`,
  thread replies with `?thread_ts=…`) or `CHANNEL/TS`, the `id` of listed
  messages. `#N` works after a list.
- Conversations: ids (`C…`, `G…`, `D…`), archive links, `#name`, or a user
  (`@name`, `U…`, an email) for your direct conversation.

## Chats

- `chats` lists the conversations the token's account is in;
  `messages CHAT [-n N] [--since 7d]` pages back through history.
- `send CHAT "text" [-a FILE …] [--reply-to MESSAGE]` (a reply goes into
  the message's thread); `comment MESSAGE "text"` replies in its thread and
  `comments MESSAGE` lists the thread.
- `edit`, `delete`, `react MESSAGE 👍` (or a name like `tada`; `--undo`).
- `members CHAT`, `user USER`, `search -t user NAME`.
- `listen [CHAT …]`: live with Socket Mode; without an app-level token it
  polls the named conversations every ten seconds.
- `search QUERY` takes Slack's own syntax (`in:#general from:@ana
  has:link after:2026-01-01`); `--sort newest`. `notifications` lists
  messages that mention you. Both need a user token.
- `download MESSAGE` saves its files.

## Other commands

`workspace`, `pins CHAT`, `pin MESSAGE [--undo]`, `bookmarks CHAT`,
`join CHANNEL`, `leave CHAT`.
