## Setup

OneBot is a protocol, not a service: run an implementation that logs in to
QQ (NapCat, Lagrange.OneBot, LLOneBot ...), enable its HTTP server and/or its
forward websocket, and point social-cli at it:

    social onebot login --token 'http://127.0.0.1:3000|ACCESS_TOKEN|ws://127.0.0.1:3001'

The parts are `URL[|ACCESS_TOKEN][|WS_URL]`. A `ws://` URL alone works for
everything (actions travel over the websocket too); an HTTP URL needs the
websocket part for `listen`. `ONEBOT_URL`, `ONEBOT_TOKEN` and `ONEBOT_WS_URL`
override the saved values for one run.

## References

- Chats: `group:<group number>` and `user:<QQ number>` (friends), or `#N`
  after `chats`.
- Messages: `CHAT/<message_id>` as listed (a bare message id works for
  `read`, `delete` and `react`).

## Commands

- `chats` lists groups and friends; `members group:N` a group's members.
- `messages CHAT` pages back through history with the `get_group_msg_history`
  / `get_friend_msg_history` extensions (NapCat, Lagrange, go-cqhttp for
  groups); implementations without them say so.
- `send CHAT TEXT [-a FILE] [--reply-to MSG]`: files go inline (`base64://`)
  as an image, video, voice or file segment by type.
- `react MSG EMOJI [--undo]` uses `set_msg_emoji_like` (NapCat, LLOneBot):
  a QQ face id or a Unicode emoji.
- `delete MSG` recalls (within QQ's two minutes, or as group admin).
- `listen [CHAT ...]`: messages (also those you send), recalls, emoji
  reactions, joins, leaves, pokes and group files.
- `kick`, `mute`, `card` administer groups; `call ACTION '{json}'` runs any
  other action of the implementation.

## Limits

QQ cannot edit messages. What works beyond the OneBot v11 core depends on
the implementation; unknown actions fail with `unsupported_operation`.
