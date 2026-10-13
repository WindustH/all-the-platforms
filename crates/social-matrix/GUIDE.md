## Homeservers

Every command talks to one homeserver: the one you logged in to, else
`MATRIX_HOMESERVER`, else matrix.org. A server name (`matrix.org`) is
followed to its client address through `/.well-known/matrix/client`.
Without an account only the public room directory answers (`rooms`,
`search -t topic`).

## References

- Rooms: `!id:server`, `#alias:server`, `https://matrix.to/#/...` links or
  `matrix:r/alias:server` URIs. `#N` works after `chats`.
- Messages: `ROOM/$event` (the `id` of listed messages) or a matrix.to link
  to the event.
- Users: `@name:server`, a matrix.to link, or `me`.

## Logging in

- `social matrix login --token 'HOMESERVER|ACCESS_TOKEN'` (Element: Settings
  → Help & About → Access Token).
- `social matrix login --token 'HOMESERVER|@you:server|PASSWORD'` logs in
  with the password and keeps the device. Pipe it to keep it out of the
  shell history: `printf '%s' 'matrix.org|@you:matrix.org|…' | social matrix
  login --token -`.
- `MATRIX_HOMESERVER` + `MATRIX_ACCESS_TOKEN` supply both for one run.

## Rooms

- `chats` lists joined rooms (most recent first, direct chats as `dm`,
  spaces as `space`, invites, unread counts); `chats SPACE` the rooms of a
  space; `rooms [WORDS] [--server S]` the public directory.
- `messages ROOM [-n N] [--since 7d]`, `send ROOM TEXT [-a FILE ...]
  [--reply-to MESSAGE]`, `comment MESSAGE TEXT` (a reply in its room),
  `edit`, `delete` (redacts), `react MESSAGE 👍 [--undo]`, `members ROOM`,
  `read MESSAGE`, `download MESSAGE`.
- `follow ROOM` / `join ROOM` joins, `follow ROOM --undo` / `leave ROOM`
  leaves.
- `search WORDS [--filter ROOM] [--sort recent|rank]` searches the rooms you
  are in (server-side; not encrypted rooms). `notifications [--type
  highlight]`, `unread`.

## Live

`listen [ROOM ...]` long-polls `/sync` and prints new messages, edits,
reactions, redactions and invites; it remembers where it stopped, so the
next run catches up on what came in between.

## Limits

No end-to-end encryption: messages of encrypted rooms show as `encrypted`
events and cannot be sent there. Media downloads use authenticated media
(Matrix 1.11 servers).
