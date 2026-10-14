Satori is a protocol: a server (Koishi with its Satori plugin, Chronocat,
LLTwoBot ...) logs in to the platforms, and ATP talks to that server.

## Logging in

Logging in to the platforms happens in the Satori server. Give ATP the
server's address:

    atp satori login --token 'http://127.0.0.1:5140/satori|TOKEN'

The token is `URL[|TOKEN][|PLATFORM:USER_ID]`:

- `URL`: the server's Satori address, `http://` or `https://`.
  `…/satori`, `…/satori/v1` and `…/satori/v1/events` all name the same
  server.
- `TOKEN`: the server's token, if it has one (sent as
  `Authorization: Bearer` and when opening the event websocket). Keep the
  empty field when an account follows: `URL||PLATFORM:USER_ID`.
- `PLATFORM:USER_ID`: the account to act as, for servers that hold several
  (the `pick` column of `atp satori logins`). Without it, `login` takes the
  first account the server offers and a note says when there are more; log
  in again with the part to pick another.

`login` checks the account with `login.get` before saving. Every command
needs this; nothing works without it.

The saved server does not expire on ATP's side: commands work while the
server runs and its account stays logged in (`atp satori logins` shows each
account's status). When the server refuses the token, commands fail with
`not_authenticated` ("the Satori server refused the token"; check the token
in the server's Satori settings), and the event websocket with "the Satori
server closed the event connection (wrong token?)".

Environment variables, read on every run and never saved:

- `SATORI_URL`: the server's address (just the URL); it replaces the saved
  server.
- `SATORI_TOKEN`: its token.
- `SATORI_PLATFORM`, `SATORI_USER_ID`: the account to act as; sent only
  when both are set.

The last three count only together with `SATORI_URL`, and no account is
picked automatically then.

## References

- Servers: `guild:<id>`; channels: the id the server gives (it may contain
  colons, e.g. `private:123`); direct chats: `user:<id>` (opened on use);
  `#N` after `chats`.
- Messages: `CHANNEL/MESSAGE` as listed, or `#N` after `messages`.
- Users: ids as the platform behind gives them (a `user:` prefix is
  accepted).

## Commands

### chats

Without a parent: servers. `chats guild:<id>` lists a server's channels.

### members

A server's members; a channel gives the members of its server.

### messages

Pages back through a channel (`message.list`), up to 100 messages per page;
`user:<id>` opens the direct chat first.

### send

Text is escaped; files go inline as `data:` resources (image, video, audio
or file elements; `.silk` voice notes as audio); `--reply-to` adds a
`<quote>`.

### edit, delete, react

Take `CHANNEL/MESSAGE`. `react --undo` removes the reaction.

### listen

`listen [CHAT|guild:ID ...]`: messages, edits, deletions, reactions, joins
and leaves. After a dropped connection it resumes from the last event, so
the server replays what was missed; after three failures in a row it stops
with the error. With an account picked, events of the server's other
accounts are left out. Direct chats are filtered by their channel id: open
one with `messages user:<id>` first.

### logins

The accounts the server offers: platform, user id, name, status, and the
`pick` value for `login --token 'URL|TOKEN|PLATFORM:USER_ID'`.

### friends

Friends of the account; `--cursor` continues from an earlier `next`.

### call

Calls any API method (`resource.method`, e.g. `guild.get`) with a JSON body,
for whatever the commands above don't cover.

## Limits

What works depends on the server and the platform behind it: methods a
platform lacks fail with `unsupported_operation`, methods it forbids with
`permission_denied`. ATP does not pace Satori requests.
