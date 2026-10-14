## Logging in

Every command talks to one Rocket.Chat server with your user id and a
token; nothing works without logging in.

### Personal access token

`atp rocketchat login --token 'https://chat.example.com|USER_ID|TOKEN'`
with a personal access token (My Account → Personal Access Tokens shows
both the token and your user id). A server without `https://` gets it
added.

### Password

`atp rocketchat login --token 'https://chat.example.com|USERNAME|PASSWORD'`
logs in with the password and saves the auth token it yields; ATP tries the
two values as a user id and token first. Accounts with 2FA need a personal
access token.

The parts may also be separated by spaces
(`'https://chat.example.com USER_ID TOKEN'`); use `|` when the password has
spaces. `-` reads the value from stdin. Either way the account is checked
before it is saved.

### When the token stops working

A personal access token works until it is revoked; how long a password
login's token lasts depends on the server. Then commands fail with
`not_authenticated`; log in again.

### Environment variables

- `ROCKETCHAT_URL`, `ROCKETCHAT_USER_ID` and `ROCKETCHAT_TOKEN`: the
  server, user id and token for one run, all three needed; never saved, and
  they win over the saved session.

## References

- Messages: links with `?msg=ID` (Copy Link in the message menu), message
  ids, or `ROOM/ID`. `#N` works after a list.
- Rooms: ids, links (`https://SERVER/channel/NAME`, `/group/NAME`,
  `/direct/…`), `#name` / `name`, or `@user` for the direct room.
- Users: ids, `@name`, `me`.

## Commands

### chats

`chats` lists the rooms you are in (channels, private groups, direct
messages, discussions; recent first); `chats TEAM` the rooms of a team.

### messages

`messages ROOM [-n N] [--since 7d]`.

### send

`send ROOM "text" [-a FILE …] [--reply-to MESSAGE]`: the reply goes into
the message's thread.

### comment, comments

`comment MESSAGE "text"` replies in its thread; `comments MESSAGE` lists
the thread.

### react

`react MESSAGE 👍`, or a name like `tada`; `--undo` removes it.

### search

`search 'terms room:general'` searches one room (`in:NAME` works too);
Rocket.Chat has no search across rooms. `search -t user NAME` finds users.

### listen

`listen [ROOM …]` streams new and edited messages of the rooms named, or of
every room you are in, over the realtime API; dropped connections are
reopened.

### unread

`unread` counts unread messages per room.

### favorite, unfavorite

`favorite MESSAGE [--undo]` stars a message.

### download

`download MESSAGE` saves its files.

### room

`room ROOM` shows a room's topic, members and message count.

### pins, pin

`pins ROOM` lists pinned messages; `pin MESSAGE [--undo]` pins or unpins.

### join, leave

`join CHANNEL` joins a public channel; `leave ROOM` leaves a room.

## Limits

Requests are spaced at least 0.2 s apart and writes wait 0.2–0.6 s first.
When the server rate-limits (429), ATP waits until the limit resets (up to
a minute, a few times) before it fails with `rate_limited`.
