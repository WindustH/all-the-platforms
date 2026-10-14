KOOK is used through a bot: ATP sees what the bot's roles allow in the
servers it was invited to.

## Logging in

### Bot token

1. Create a bot at https://developer.kookapp.cn/app and copy its token.
2. Invite the bot to your servers and give its roles the permissions you
   need.
3. Run `atp kook login --token 'BOT_TOKEN'`.

The token is checked against `/user/me` before it is kept, and goes out as
`Authorization: Bot <token>`; a value that already starts with `Bot ` or
`Bearer ` is sent as given. Every command needs it; nothing works without
it. `KOOK_TOKEN` supplies a token for one run (it wins over the saved one
and is never written).

How long a bot token stays valid is not known. When KOOK no longer accepts
it, commands fail with `not_authenticated` ("KOOK did not accept the
token"; check the token at https://developer.kookapp.cn/app), and `listen`
with "KOOK refused the connection". What the bot's roles do not allow fails
with `permission_denied`.

## References

- Servers: `guild:<id>` (also `server:`); channels: their bare id; direct
  chats: `dm:<user id>` (also `user:`); `#N` after `chats`.
- Messages: `CHAT/<message id>` as listed, or `#N` after `messages`. A bare
  message id (a UUID) is taken as a channel message; direct messages need
  the `dm:<user id>/` part.
- Users: numeric ids (a `user:` or `dm:` prefix is accepted).

## Commands

### chats

Without a parent: servers, then direct chats. `chats guild:<id>` (or a
channel id of that server) lists a server's channels.

### members

Takes a server or one of its channels; either way the server's members.

### messages

Pages back through a channel or a direct chat, up to 100 messages per
page. A server has channels, not messages.

### send

Text goes as KMarkdown (so `edit` can change it later); each file is
uploaded as an asset and sent as its own image / video / audio / file
message. `--reply-to` quotes the message on every part. Send to a channel
or a direct chat, not to a server.

### edit

Only KMarkdown and card messages can be edited.

### react

Takes an emoji or a server emoji id; `--undo` removes the reaction.

### listen

Messages, edits, deletions, reactions (added and removed), joins and leaves
as they happen, through KOOK's websocket gateway. Filter by channel ids,
`dm:<user id>`, or `guild:<id>` for every channel of a server (its joins
and leaves too). A closed connection is re-opened; after three failed
connections in a row `listen` stops with the error.

### server, roles

A server's details and its roles; both take `guild:<id>`.

### upload

Uploads a file as a KOOK asset and prints the reply, with its asset `url`.

## Limits

Bots cannot search messages. Requests are spaced at least 200 ms apart.
When KOOK answers 429, ATP waits for the time it asks (`X-Rate-Limit-Reset`,
0.5 to 30 seconds) and retries, three tries in all; then the command fails
with `rate_limited`.
