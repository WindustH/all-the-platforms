A QQ official bot (QQ 机器人), not a QQ account: its 频道 servers and
sub-channels, 私信, groups and single (C2C) chats, through QQ's bot API.
Bots get no message history; messages arrive through `listen`.

## Logging in

1. Create a bot at https://q.qq.com and copy its AppID and AppSecret
   (开发 → 开发设置). The AppID is a number.
2. Add this machine's public IP to the bot's IP whitelist there. Without it
   QQ issues no access token.
3. Run:

       atp qq login --token 'APPID:SECRET'

   `APPID:SECRET:sandbox` uses QQ's sandbox environment
   (`sandbox.api.sgroup.qq.com`) instead.

`login` buys an access token with the credentials before it saves them; a
refusal reads "QQ did not issue an access token" with QQ's reason and the
hint to check the AppID, AppSecret and IP whitelist.

The access token lasts about two hours. ATP caches it privately, fetches a
new one shortly before it expires, and once more when QQ answers HTTP 401.
The AppID and AppSecret do not expire here; when QQ refuses them, commands
fail with `not_authenticated` and the hint to check them.

Environment variables (they win over the saved login for that run):

- `QQ_BOT_APPID` and `QQ_BOT_SECRET`: the bot's credentials (both needed).
- `QQ_BOT_SANDBOX=1`: use the sandbox (any value but `0`).
- `QQ_BOT_INTENTS=<bitmask>`: the event groups `listen` asks for (see
  `atp guide qq listen`).

Nothing works without logging in.

## References

- Servers (频道): `guild:<id>` (or `server:<id>`), from `chats`, or `#N`
  after `chats`.
- Sub-channels: their bare id (or `channel:<id>`), from `chats guild:<id>`.
- 私信 sessions: `dm:<guild id>`, as `open-dm` prints it and as `listen`
  shows received 私信.
- Groups and single chats: `group:<group_openid>` and `user:<user_openid>`
  (or `c2c:<openid>`). QQ gives bots these openids only in events, so take
  them from `listen`.
- Messages: `CHAT/<message id>`, as `listen` prints them.
- Server members: `SERVER/USER` (`guild:<id>/<user id>`) for `user`.

## Commands

### chats

Without an argument: the bot's servers. With a server (`guild:<id>`) or one
of its sub-channels: that server's sub-channels.

### members

Members of a server: give `guild:<id>` or one of its sub-channels.

### user

`user SERVER/USER`: bots look users up within a server, so name both.

### read, download

Only sub-channel messages can be looked up (`CHANNEL/MESSAGE`); group, C2C
and 私信 messages exist only as `listen` printed them.

### send

`send CHAT TEXT [-a FILE] [--reply-to CHAT/MSG]` to a sub-channel, a
`dm:` session, a `group:` or a `user:` chat (not to a `guild:`).

- A reply naming a received message is *passive*: QQ allows a few per
  message within minutes (about 5 minutes in groups and sub-channels, 60 in
  C2C). `--reply-to event:<event_id>` answers a non-message event (such as
  the bot being added to a group) the same way; `listen` prints the
  `event_id`.
- Without a reply the message is *active*, which QQ rations heavily or
  refuses; ATP says "sent as an active message".
- Groups and C2C take images, video, voice (`.silk` or audio) and files,
  uploaded as rich media first. Sub-channels and 私信 take images only,
  one image per message.
- Messages QQ holds for review count as sent.

### delete

Recalls a message in any kind of chat.

### react

Sub-channel messages only. A number adds a QQ system face by its id; any
other value adds its first emoji. `--undo` takes the reaction back.

### listen

Prints sub-channel @-mentions, 私信, group @-mentions and C2C messages,
recalls, reactions, the bot being added to or removed from groups and
friend lists, and members joining or leaving servers. `listen CHAT…` keeps
only those chats.

`listen` asks for the event groups bots get by default plus group / C2C,
私信 and reaction events. QQ closes the connection for groups a bot was not
granted, and fewer are tried; when even the basic set is refused, `listen`
fails with `not_authenticated` ("check the bot's event permissions").
`QQ_BOT_INTENTS=<bitmask>` sets them exactly (e.g. private-domain bots can
add GUILD_MESSAGES, 1<<9). Dropped connections are reopened; after more
than 20 drops `listen` gives up.

### open-dm

`open-dm USER SERVER` opens a 私信 session with a member of a server the
bot is in and prints its `dm:` chat, which `send` then takes.

### server, roles

Take a server as `guild:<id>` and print QQ's raw details.

### pins, pin

Sub-channels only: `pins CHANNEL` lists pinned messages as
`CHANNEL/MESSAGE`; `pin CHANNEL/MESSAGE [--undo]`.

## Limits

- No message history, no message search, no edits.
- ATP keeps at least 200 ms between requests. QQ's HTTP 429 and codes
  22009 / 40054005 come back as `rate_limited`, HTTP 403 as
  `permission_denied`.
