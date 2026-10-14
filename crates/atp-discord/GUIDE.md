## Logging in

ATP talks to Discord's REST API with a user token, as the web client does,
or with a bot token. The cookie options do not apply.

### QR code

`atp discord login` shows a QR code: scan it with the Discord app
(Settings → Scan QR Code) and confirm. This logs in your user account.
Declining on the phone, or letting the code expire, ends the login with an
error; run it again.

### Token

`atp discord login --token TOKEN` takes a user token or a bot token
(`Bot TOKEN` or `bot:TOKEN` to say which; otherwise both are tried, user
first). The token is checked against the API before it is saved. `-` reads
it from stdin, which keeps it out of the shell history.

Bots are created at https://discord.com/developers/applications and invited
to your servers. They need the Message Content intent for message text in
`listen`, and the Server Members intent for `members SERVER`.

### What each login can do

A user account can do everything below. Bots cannot list direct chats,
search messages, read mentions (`notifications`) or see friends, and see
only the basics of user profiles. User accounts cannot list the members of
large servers unless they moderate them.

Without logging in only `invite CODE` works.

### Risks

Automating a user account is against Discord's terms of service and can get
the account limited or banned; keep it light, and prefer a bot for anything
regular. User-account requests carry the web client's headers, and each
write waits 0.4–1.2 s first.

Discord may ask a user account for a captcha before an action
(`verification_required`): do the same once in the Discord app or website,
then retry; bots never get captchas. When Discord wants the account's email
or phone verified, do that in the app.

### When the token stops working

How long a token lasts is not known; a user token is reset by a password
change or a logout in Discord. Then commands fail with `not_authenticated`
("Discord did not accept the token"); log in again.

### Environment variables

- `DISCORD_BOT_TOKEN`: a bot token for one run, never saved.
- `DISCORD_TOKEN`: a user token, or `Bot …` for a bot, for one run.

Either wins over the saved session; `DISCORD_BOT_TOKEN` wins over
`DISCORD_TOKEN`.

## References

- Messages: links (`https://discord.com/channels/SERVER/CHANNEL/MESSAGE`,
  also ptb / canary / discordapp.com), `CHANNEL/MESSAGE` (the `id` of
  listed messages) or Discord's "Copy Message ID" form `CHANNEL-MESSAGE`.
- Chats: channel and server ids or links, `<#CHANNEL>`, `server:ID`,
  `channel:ID`; a user (`user:ID`, `<@ID>`, `@ID` or a profile link) means
  your direct chat with them. `#N` works after `chats`.
- Users: ids, `<@ID>`, `user:ID`, `discord.com/users/ID`; `me`. Discord
  looks users up by id only.
- `server`, `roles`, `emojis`, `invites`, `pins` and `pin` take ids and
  links, not `#N`.

## Commands

### chats

`chats` lists your direct chats (most recent first) and servers;
`chats SERVER` its channels by category, `chats CHANNEL` the threads of a
text or forum channel. Bots get no direct chats; for threads they see the
server's active ones and a channel's archived public ones, user accounts a
channel's active and archived ones.

### messages

`messages CHANNEL [-n N] [--since 7d]` pages back through history, 100
messages a request. Long exports: `-f jsonl` or `-f csv`.

### send

`send CHAT "text" [-a FILE ...] [--reply-to MESSAGE]`; a user as CHAT opens
your direct chat. User accounts send at most 2000 characters a message
(also for `edit`). A file too large for the channel is refused.

### edit, delete, react

`react MESSAGE 👍` with `--undo`; custom emoji as `name:id` (or
`<:name:id>`), see `emojis SERVER`.

### comment, comments, delete-comment

`comment MESSAGE "text"` replies to a message in its channel;
`comments MESSAGE` lists the thread started from it (or the forum post it
opens).
`delete-comment MESSAGE REPLY` takes the reply's link, `CHANNEL/MESSAGE`,
or its bare id in the same channel.

### members

`members SERVER` lists a server's members (bots with the Server Members
intent, up to 1000 a page; user accounts only as moderators);
`members CHAT` the people of a direct or group chat.

### listen

`listen [CHAT|SERVER ...]` streams new messages, edits, deletions and
reactions; `-f jsonl` gives one JSON object per event. Bots without the
Message Content intent still listen, with text only for mentions and
direct messages. For user accounts the servers of the chats named are
subscribed, so large servers send their messages too. Lost connections
reconnect.

### search

User accounts only, as in the app: scope the query with `server:ID` or
`in:CHANNEL`, and narrow it with `from:USER`, `mentions:USER`, `has:…`,
`pinned:true`, `before:` / `after:` (a date or `7d`):

    atp discord search 'release notes server:123 from:456 after:30d'
    atp discord search 'in:789 has:image' --sort oldest

`--filter` is `has:`; `--sort` is `newest`, `oldest` or `relevance`.
Results come 25 a page; Discord stops paging at about 10 000.
`search -t user 'ali server:123'` finds members by name; it needs the
server, as Discord has no global user search.

### user

User accounts see bio, connections and mutual servers; bots, and profiles
without a shared server or friendship, give the basics.

### notifications

`notifications [--type direct]` lists mentions: `all` (mentions, role and
@everyone pings) or `direct` (only you by name). User accounts only.

### download

`download MESSAGE` saves attachments and embedded media.

### server

`server SERVER` shows counts, boosts, features, channels and roles.

### roles, emojis

`roles SERVER` lists roles, highest first; `emojis SERVER` the custom
emoji with the `name:id` form `react` takes.

### invite, invites

`invite CODE` (a code or a discord.gg / discord.com/invite link) shows what
an invite leads to and works without logging in. `invites SERVER` lists open
invites and needs the Manage Server permission.

### pins, pin

`pins CHANNEL` lists pinned messages, `pin MESSAGE [--undo]` pins or
unpins.

### friends

`friends [--all]` lists friends and friend requests (`--all` adds blocked
accounts). User accounts only.

## Limits

Discord's rate limits are honored: ATP waits as long as Discord says (up to
a minute, a few times) before it fails with `rate_limited`, and keeps at
least 0.8 s between requests.
