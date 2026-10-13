## References

- Messages: links (`https://discord.com/channels/SERVER/CHANNEL/MESSAGE`,
  also ptb / canary / discordapp.com), `CHANNEL/MESSAGE` (the `id` of
  listed messages) or Discord's "Copy Message ID" form `CHANNEL-MESSAGE`.
- Chats: channel and server ids or links, `<#CHANNEL>`, `server:ID`,
  `channel:ID`; a user (`user:ID`, `<@ID>`, `@ID` or a profile link) means
  your direct chat with them. `#N` works after `chats`.
- Users: ids, `<@ID>`, `user:ID`, `discord.com/users/ID`; `me`.

## Logging in

- `social discord login` shows a QR code: scan it with the Discord app
  (Settings → Scan QR Code) and confirm.
- `social discord login --token TOKEN` takes a user token or a bot token
  (`Bot TOKEN` to say which; otherwise both are tried). `-` reads it from
  stdin, which keeps it out of the shell history.
- `DISCORD_BOT_TOKEN`, or `DISCORD_TOKEN` (`Bot …` for bots), supply a token
  for one run without saving it. The cookie options do not apply.

Automating a user account is against Discord's terms of service and can get
the account limited or banned; keep it light, and prefer a bot (created at
https://discord.com/developers/applications and invited to your servers)
for anything regular. Bots cannot list direct chats, search messages, read
mentions or see friends; user accounts cannot list the members of large
servers unless they moderate them.

## Chats

- `chats` lists your direct chats (most recent first) and servers;
  `chats SERVER` its channels by category, `chats CHANNEL` the threads of a
  text or forum channel.
- `messages CHANNEL [-n N] [--since 7d]` pages back through history, 100
  messages a request. Long exports: `-f jsonl` or `-f csv`.
- `send CHAT "text" [-a FILE ...] [--reply-to MESSAGE]` (a user as CHAT opens
  your direct chat); `edit`, `delete`, `react MESSAGE 👍` (custom emoji as
  `name:id`, see `emojis SERVER`) with `--undo`.
- `comment MESSAGE "text"` replies to a message in its channel; `comments
  MESSAGE` lists the thread started from it (or the forum post it opens).
- `members SERVER` (bots, with the Server Members intent) or `members CHAT`
  for the people of a direct or group chat.
- `listen [CHAT|SERVER ...]` streams new messages, edits, deletions and
  reactions; `-f jsonl` gives one JSON object per event. Bots need the
  Message Content intent for message text.

## Search

User accounts only, as in the app: scope the query with `server:ID` or
`in:CHANNEL`, and narrow it with `from:USER`, `mentions:USER`, `has:…`,
`pinned:true`, `before:` / `after:` (a date or `7d`):

    social discord search 'release notes server:123 from:456 after:30d'
    social discord search 'in:789 has:image' --sort oldest

`--filter` is `has:`; `--sort` is `newest`, `oldest` or `relevance`.
`search -t user 'ali server:123'` finds members by name.

## Other commands

`server SERVER` (counts, boosts, features), `roles`, `emojis`, `invites
SERVER`, `invite CODE` (works without logging in), `pins CHANNEL`, `pin
MESSAGE [--undo]`, `friends [--all]`, `notifications [--type direct]`
(mentions), `download MESSAGE` (attachments and embedded media).
