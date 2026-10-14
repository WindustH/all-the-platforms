## Logging in

### Homeserver

Every command talks to one homeserver: the one you logged in to, else
`MATRIX_HOMESERVER`, else matrix.org. A server name (`matrix.org`) is
followed to its client address through `/.well-known/matrix/client`.

### Access token

`atp matrix login --token 'HOMESERVER|ACCESS_TOKEN'` (Element: Settings →
Help & About → Access Token). A bare `--token ACCESS_TOKEN` keeps the
homeserver in use.

### Password

`atp matrix login --token 'HOMESERVER|@you:server|PASSWORD'` logs in with
the password and keeps the device: it shows as "all-the-platforms" in your
sessions, and later password logins reuse it. Pipe it to keep it out of the
shell history:

    printf '%s' 'matrix.org|@you:matrix.org|…' | atp matrix login --token -

The parts are separated by `|` only; the password may contain `|` itself.
The token is checked before it is saved.

### Without an account

Only the public room directory answers (`rooms`, `search -t topic`), and
`user` looks up profiles without a token where the homeserver allows it.

### When the token stops working

How long an access token lasts is up to the homeserver; when it is no
longer valid, commands fail with `not_authenticated`; log in again.

### Environment variables

- `MATRIX_HOMESERVER`: the homeserver to use when none is saved.
- `MATRIX_ACCESS_TOKEN`: an access token for one run, never saved; it wins
  over the saved one.

## References

- Rooms: `!id:server`, `#alias:server`, `https://matrix.to/#/...` links or
  `matrix:r/alias:server` URIs. `#N` works after `chats`.
- Messages: `ROOM/$event` (the `id` of listed messages) or a matrix.to link
  to the event.
- Users: `@name:server`, a matrix.to link, or `me`.

## Commands

### chats

`chats` lists joined rooms (most recent first, direct chats as `dm`,
spaces as `space`, invites, unread counts); `chats SPACE` the rooms of a
space.

### rooms

`rooms [WORDS] [--server S]` lists the public directory. With words a
logged-in homeserver searches it; without an account ATP filters the first
pages itself.

### messages

`messages ROOM [-n N] [--since 7d]`.

### delete

`delete MESSAGE` redacts it.

### comment

`comment MESSAGE TEXT` sends a reply in the message's room.

### follow, unfollow, join, leave

`follow ROOM` / `join ROOM` joins, `follow ROOM --undo` / `unfollow ROOM` /
`leave ROOM` leaves.

### search

`search WORDS [--sort recent|rank]` searches the rooms you are in
(server-side; not encrypted rooms). `--filter ROOM` limits it to one room.
`search -t topic WORDS` searches the public room directory.

### notifications, unread

`notifications [--type highlight]` lists messages that notified you
(`highlight` keeps mentions and keywords); `unread` counts per room.

### listen

`listen [ROOM ...]` long-polls `/sync` and prints new messages, edits,
reactions, redactions and invites; it remembers where it stopped (for a
week), so the next run catches up on what came in between.

## Limits

No end-to-end encryption: messages of encrypted rooms show as `encrypted`
events and cannot be sent there. Media downloads use authenticated media
(Matrix 1.11 servers). Requests are spaced at least 0.3 s apart and writes
wait 0.2–0.7 s first; when the homeserver rate-limits, ATP waits as it says
(up to 30 s, a few times) before it fails with `rate_limited`.
