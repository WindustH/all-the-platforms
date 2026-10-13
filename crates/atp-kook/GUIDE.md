## Setup

KOOK is used through a bot: create one at https://developer.kookapp.cn/app,
invite it to your servers, copy its token and run
`atp kook login --token '...'` (or set `KOOK_TOKEN`). The bot sees what
its roles allow.

## References

- Servers: `guild:<id>`; channels: their id; direct chats: `dm:<user id>`;
  `#N` after `chats`.
- Messages: `CHAT/<message id>` as listed.

## Commands

- `chats` lists servers and direct chats; `chats guild:<id>` a server's
  channels. `members` takes a server or one of its channels.
- `messages CHAT` pages back through a channel or a direct chat.
- `send CHAT TEXT [-a FILE] [--reply-to MSG]`: text goes as KMarkdown (so
  `edit` can change it later); each file is uploaded and sent as its own
  image / video / audio / file message.
- `react MSG EMOJI [--undo]` takes an emoji or a server emoji id.
- `listen [CHAT ...]`: messages, edits, deletions, reactions, joins and
  leaves as they happen.
- `server`, `roles`, `upload FILE` (asset URL).

## Limits

Bots cannot search messages. Only KMarkdown and card messages can be edited.
