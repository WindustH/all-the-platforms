## Setup

Satori is a protocol: a server (Koishi with its Satori plugin, Chronocat,
LLTwoBot ...) logs in to the platforms and all-the-platforms talks to it.

    atp satori login --token 'http://127.0.0.1:5140/satori|TOKEN'

The parts are `URL[|TOKEN][|PLATFORM:USER_ID]`. Without the last part the
first account the server offers is used; `atp satori logins` lists them.
`SATORI_URL`, `SATORI_TOKEN`, `SATORI_PLATFORM` and `SATORI_USER_ID`
override the saved values for one run.

## References

- Servers: `guild:<id>`; channels: the id the server gives (it may contain
  colons, e.g. `private:123`); direct chats: `user:<id>` (opened on use).
- Messages: `CHANNEL/MESSAGE` as listed.

## Commands

- `chats` lists servers, `chats guild:<id>` their channels, `members` a
  server's members (a channel's server too).
- `messages CHAT` pages back through a channel (`message.list`).
- `send CHAT TEXT [-a FILE] [--reply-to MSG]`: text is escaped; files go
  inline as `data:` resources; replies as a `<quote>`.
- `edit`, `delete`, `react MSG EMOJI [--undo]`, `user`, `read`.
- `listen [CHAT|guild:ID ...]`: messages, edits, deletions, reactions,
  joins and leaves; reconnects resume from the last event.
- `logins`, `friends`, `call resource.method '{json}'` for anything else.

## Limits

What works depends on the server and the platform behind it: methods a
platform lacks fail with `unsupported_operation`.
