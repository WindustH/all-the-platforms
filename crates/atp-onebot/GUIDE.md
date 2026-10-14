OneBot is a protocol, not a service: run an implementation that logs in to
QQ (NapCat, Lagrange.OneBot, LLOneBot, go-cqhttp ...) and point ATP at it.

## Logging in

Logging in to QQ happens in the implementation. There, enable its HTTP
server and/or its forward websocket, then give ATP the address:

    atp onebot login --token 'http://127.0.0.1:3000|ACCESS_TOKEN|ws://127.0.0.1:3001'

The token is `URL[|ACCESS_TOKEN][|WS_URL]`:

- `URL`: where actions go. An `http://` / `https://` address takes each
  action as `POST URL/<action>`; a `ws://` / `wss://` address is the
  forward websocket, which takes actions too and carries the events
  `listen` prints. So a `ws://` URL alone works for everything.
- `ACCESS_TOKEN`: the access token set in the implementation, sent as
  `Authorization: Bearer`. Leave it out when none is set (keep the empty
  field when a `WS_URL` follows: `http://127.0.0.1:3000||ws://127.0.0.1:3001`).
- `WS_URL`: the forward websocket, needed with an HTTP `URL` for `listen`
  only.

`login` tries the address with `get_login_info` before saving it, and
`whoami` shows the QQ account the implementation is logged in to. Every
command needs this; nothing works without it.

The saved address does not expire on ATP's side: commands work while the
implementation runs and stays logged in to QQ (how long that lasts depends
on the implementation). When the implementation refuses the access token,
commands fail with `not_authenticated` ("the OneBot implementation refused
the access token"; check the token in the implementation's settings); when
it is not running, with a network error.

Environment variables, read on every run and never saved:

- `ONEBOT_URL`: an address in the same `URL[|ACCESS_TOKEN][|WS_URL]` form;
  it replaces the saved one.
- `ONEBOT_TOKEN`, `ONEBOT_WS_URL`: the access token and the websocket,
  overriding those parts of `ONEBOT_URL`. They count only together with
  `ONEBOT_URL`.

## References

- Chats: `group:<group number>` (also `g:`) and `user:<QQ number>` for
  friends (also `u:`, `private:`, `friend:`), or `#N` after `chats`.
- Messages: `CHAT/<message_id>` as listed, or `#N` after `messages`. A bare
  message id works for `read`, `delete`, `react` and `--reply-to`.
- Users: QQ numbers (a `user:` prefix is accepted), or `#N` after `members`.

## Commands

### chats

Groups first, then friends. Both lists come whole from the implementation
and are paged locally.

### members

Takes a group (`group:N`); friends have no member list.

### messages

Pages back through history with the `get_group_msg_history` /
`get_friend_msg_history` extensions (NapCat, Lagrange, go-cqhttp for
groups), up to 50 messages per request; implementations without them fail
with `unsupported_operation`.

### send

Files go inline (`base64://`), so the implementation need not see the path:
images and GIFs as image segments, videos (also `.avi`) as video, audio
(also `.silk`) as voice (`record`), anything else as a file segment.
`--reply-to` adds a reply segment.

### react

Uses `set_msg_emoji_like` (NapCat, LLOneBot). EMOJI is a QQ face id
(digits) or a Unicode emoji (sent as its code point).

### delete

Recalls the message: within QQ's two minutes, or as group admin.

### download

Saves the images, voice messages, videos and files of a message that
carry a URL.

### listen

Messages (also those you send), recalls, emoji reactions, joins, leaves,
pokes and group files. It needs the forward websocket (a `ws://` URL, the
`WS_URL` part or `ONEBOT_WS_URL`); a lost connection is re-opened.

### kick, mute, card

Administer a group (`group:N`) member (QQ number or `#N`). `kick --reject`
also refuses their future join requests; `mute --minutes 0` lifts a mute;
`card` with an empty text clears the member's group card.

### call

Runs any other action of the implementation with JSON parameters, and
prints its `data`.

## Limits

QQ cannot edit messages. What works beyond the OneBot v11 core depends on
the implementation; unknown actions fail with `unsupported_operation`
(NapCat and Lagrange.OneBot implement the most extensions). ATP does not
pace OneBot requests; an action over the websocket fails when no answer
comes within 30 seconds.
