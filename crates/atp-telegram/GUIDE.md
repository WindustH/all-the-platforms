Telegram works three ways: as a user account (MTProto, like the apps), as
a bot (Bot API), or with no account (public channels through their web
preview). Each command takes the route the session allows; what a bot or no
account cannot do fails with a pointer to `atp telegram login`.

## Logging in

### User account: QR code

`atp telegram login` shows a QR code: scan it in the Telegram app under
Settings → Devices → Link Desktop Device. A code is valid for about 30
seconds; when it expires a new one is drawn in its place. With two-step
verification on, the password is asked on the terminal (with its hint).

### User account: phone number

`atp telegram login-phone [+447700900123]` logs in with the phone number
(international format with `+`; asked when left out) and the code Telegram
sends to the app or by SMS, then the two-step verification password if one
is set. The questions must be answered on a terminal; in a pipe or script
they fail. A number without a Telegram account is refused: sign up in the
app first.

### App identity

A user account unlocks everything below: chats, history, search, comments,
members, sending, `listen`, `insights`. It needs an app identity: set
`TELEGRAM_API_ID` (a number) and `TELEGRAM_API_HASH` from
<https://my.telegram.org>, both or neither. Without them ATP uses Telegram
Desktop's public identity, as tg-cli does; Telegram tolerates that, but its
terms ask apps for their own. The login shows up in the app's device list
as "all-the-platforms"; `logout` forgets it here — end it in the app.

### Bot

`atp telegram login --token 123456:ABC…` takes a token from @BotFather:
the bot's numeric id, `:`, and its secret (at least 20 characters); it is
checked with Telegram before it is saved. `TELEGRAM_BOT_TOKEN` supplies one
without saving it. Bots send, edit, delete, react, pin, forward, look up
chats (`user`) and their admins (`members`), and `listen`; the Bot API gives
them no chat list, history or search. `messages` and `read` of public
channels still work through the web preview.

### Without an account

Public channels through their web preview: `messages` (`user-posts`),
`read`, `user`, `search-chat`, `listen` and `download`. Groups and private
chats have no preview.

### Which login is used

A saved bot token comes first, then a saved user account, then
`TELEGRAM_BOT_TOKEN`, else no account. A user login removes a saved bot
token and `login --token` removes a saved user account.

### When the session ends

How long a user session lasts is up to Telegram; ending it in the app's
device list ends it here too. Then commands fail with `not_authenticated`
("the session ended; run `atp telegram login`"). A refused bot token fails
the same way, with the hint to get a new one from @BotFather.

### Environment variables

- `TELEGRAM_BOT_TOKEN`: a bot token, used when no session is saved; never
  written.
- `TELEGRAM_API_ID`, `TELEGRAM_API_HASH`: the app identity for user
  accounts (see above).

## References

- Chats: `@name`, a `t.me/name` link (also `t.me/s/name`, telegram.me,
  `tg://resolve?domain=name`), an invite link (`t.me/+…`,
  `t.me/joinchat/…`), a private `t.me/c/<id>` link, or the numeric id
  `chats` shows (Bot API form: users positive, groups negative, channels
  `-100…`). `me` or `self` is your own account: your Saved Messages with a
  user login, the bot itself for `user me` with a bot token (bots have no
  Saved Messages), and not available without an account. Numeric ids work
  once the session has seen the chat (after `chats`).
- Messages: `t.me/name/42`, `t.me/c/<id>/42`, `t.me/name/42?comment=7`,
  `tg://privatepost?channel=…&post=…`, or `CHAT/42` — the `id` every listed
  message carries.
- Forum topics: `chats GROUP` lists them; `messages GROUP/TOPIC` reads one.

## Commands

### chats

`chats` lists dialogs with unread counters; `chats GROUP` lists a forum
group's topics. User accounts only.

### messages, user-posts

`messages CHAT [-n 500] [--since 7d]` reads history newest first
(`-f jsonl` for exports). Bots and no account read public channels through
the web preview instead.

### read

A `?comment=7` link reads that comment under a channel post (user
accounts). Bots and no account read public channel posts through the web
preview.

### send

`send CHAT "text" [-a FILE …] [--reply-to MSG]`; several files go as an
album. JPEG, PNG and WebP files go as photos, everything else (GIFs
included) as documents. Bots cannot send to invite links; use the chat's id
or @name.

### edit, delete, react

User accounts and bots; `react MSG 👍 [--undo]`.

### pin

`pin MSG [--undo]` pins or unpins, as a user account or a bot.

### forward

`forward MSG CHAT` forwards a message to another chat.

### search

`search WORDS [--filter photo|video|media|document|url|voice|music|gif|round|mention]`
searches all chats; `search -t user NAME` finds public people, groups and
channels by name, and matching contacts. User accounts only.

### search-chat

`search-chat CHAT WORDS [--filter …]` searches one chat. Without a user
account it searches a public channel's web preview, which ignores
`--filter`.

### comments

`comments POST` reads a channel post's comments. User accounts only.

### members

`members CHAT` lists the members; bots get the administrators only, with
their role and custom title.

### follow, unfollow

`follow CHAT` joins a channel or supergroup, or any group by its invite
link; `--undo` or `unfollow` leaves a channel or supergroup. `#N` is a
chat of the last chat list (`chats`). User accounts only.

### unread, mark-read

`unread` counts unread messages per chat; `mark-read CHAT` marks one read.
User accounts only.

### listen

`listen [CHAT …]` prints events as they come, by `kind`:

- user accounts: `message`, `edit` and `delete`; deletions in private chats
  and basic groups do not say where, so a chat filter drops them;
- bots: `message`, `edit` and `reactions`; a bot with a webhook must remove
  it (`deleteWebhook`) before it can listen here;
- no account: new posts of the public channels named (`@name`), checked
  every 30 seconds.

### insights

Posts carry views, forwards (`shares`), comment counts and reactions
(`metrics.likes` is their total; `extra.reactions` lists them).

`insights @channel` shows the statistics Telegram keeps for channels and
supergroups you administer (Telegram enables them for larger ones):
followers / members with the previous period, views, shares and reactions
per post, notification rate, growth and interaction series, and breakdowns
by source, language and reaction. `insights CHANNEL/42` covers one post.
User accounts only.

### download

`download MSG` saves photos, videos, voice notes and documents in their
original quality. Bots fetch files up to 20 MB (a Bot API limit).

### login-phone

Logs in a user account with its phone number; see
`atp guide telegram login`.

## Limits

- Where Telegram is blocked, the account connection goes through `--proxy`
  or the proxy variables (`ATP_PROXY`, `ALL_PROXY`, `HTTPS_PROXY`). SOCKS5
  proxies are used directly; HTTP proxies through a local bridge.
- Telegram answers floods with waits (`rate_limited`); short waits are
  slept through.
- Bots cannot read history; the web preview shows the last posts of public
  channels only.
- Bot API and web preview requests are spaced at least 0.5 s apart.
