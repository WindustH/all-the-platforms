## Logging in

- `atp rocketchat login --token 'https://chat.example.com USER_ID TOKEN'`
  with a personal access token (My Account → Personal Access Tokens shows
  both the token and your user id), or
  `--token 'https://chat.example.com USERNAME PASSWORD'` for a password
  login (its auth token is saved; accounts with 2FA need a personal access
  token). Separate the parts with `|` when the password has spaces; `-`
  reads the value from stdin.
- `ROCKETCHAT_URL`, `ROCKETCHAT_USER_ID` and `ROCKETCHAT_TOKEN` supply all
  three for one run.

## References

- Messages: links with `?msg=ID` (Copy Link in the message menu), message
  ids, or `ROOM/ID`. `#N` works after a list.
- Rooms: ids, links (`https://SERVER/channel/NAME`, `/group/NAME`,
  `/direct/…`), `#name` / `name`, or `@user` for the direct room.
- Users: ids, `@name`, `me`.

## Chats

- `chats` lists the rooms you are in (channels, private groups, direct
  messages, discussions; recent first); `chats TEAM` the rooms of a team.
- `messages ROOM [-n N] [--since 7d]`, `send ROOM "text" [-a FILE …]
  [--reply-to MESSAGE]` (the reply goes into the message's thread),
  `comment MESSAGE "text"`, `comments MESSAGE` (its thread), `edit`,
  `delete`, `react MESSAGE 👍` (or a name like `tada`; `--undo`).
- `members ROOM`, `user USER`, `search -t user NAME`.
- `listen [ROOM …]` streams new and edited messages of the rooms named, or
  of every room you are in.
- `search 'terms room:general'` searches one room; `unread` counts per room;
  `favorite MESSAGE [--undo]` stars a message.
- `download MESSAGE` saves its files.

## Other commands

`room ROOM`, `pins ROOM`, `pin MESSAGE [--undo]`, `join CHANNEL`,
`leave ROOM`.
