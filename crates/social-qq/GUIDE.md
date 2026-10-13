## Setup

Create a bot at https://q.qq.com, copy its AppID and AppSecret (开发 →
开发设置), add this machine's IP to the bot's IP whitelist, and run:

    social qq login --token 'APPID:SECRET'

Append `:sandbox` to use the sandbox environment. `QQ_BOT_APPID`,
`QQ_BOT_SECRET` and `QQ_BOT_SANDBOX=1` override the saved values for one
run. The access token is fetched and renewed automatically.

## References

- Servers (频道): `guild:<id>`; sub-channels: their id; 私信 sessions:
  `dm:<guild id>` (`open-dm USER SERVER` opens one).
- Groups and single chats: `group:<group_openid>` and `user:<user_openid>`.
  QQ gives bots these openids only in events, so take them from `listen`.
- Messages: `CHAT/<message id>`, as `listen` prints them.

## What a bot can do

- `chats` lists the bot's servers, `chats guild:<id>` their sub-channels;
  `members SERVER`; `user SERVER/USER`.
- There is **no message history** for bots. Messages arrive through
  `listen` (sub-channel @-mentions, 私信, group @-mentions, C2C messages,
  deletions, reactions, bot added / removed); `read` looks up one
  sub-channel message.
- `send CHAT TEXT [-a FILE] [--reply-to CHAT/MSG]`. A reply naming a
  received message is *passive*: QQ allows a few per message within minutes
  (about 5 minutes in groups and sub-channels, 60 in C2C). `--reply-to
  event:<event_id>` answers a non-message event the same way. Without a
  reply the message is *active*, which QQ rations heavily or refuses.
  Groups and C2C take images, video, voice (silk) and files; sub-channels
  and 私信 take images.
- `delete` recalls; `react` adds a system face (number) or an emoji in
  sub-channels; `pins`, `pin MSG [--undo]`, `server`, `roles`.

## Events

`listen` asks for the event groups bots get by default plus group / C2C,
私信 and reaction events; QQ closes the connection for groups a bot was not
granted, and fewer are tried. `QQ_BOT_INTENTS=<bitmask>` sets them exactly
(e.g. private-domain bots can add GUILD_MESSAGES, 1<<9).
