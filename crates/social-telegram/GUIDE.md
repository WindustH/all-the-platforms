## Three ways in

- **A user account** (MTProto, like the apps): everything below.
  `social telegram login` shows a QR code: scan it in the Telegram app under
  Settings → Devices → Link Desktop Device. With two-step verification on,
  the password is asked on the terminal. `social telegram login-phone`
  logs in with the phone number and the code Telegram sends instead.
- **A bot** (Bot API): `social telegram login --token 123456:ABC…` with a
  token from @BotFather, or `TELEGRAM_BOT_TOKEN`. Bots send, edit, delete,
  react, pin, forward, look up chats and their admins, and `listen`; the
  Bot API gives them no chat list, history or search.
- **No account**: public channels through their web preview — `messages`
  (`user-posts`), `read`, `user`, `search-chat`, `listen` and `download`.

The user account needs an app identity: set `TELEGRAM_API_ID` and
`TELEGRAM_API_HASH` from <https://my.telegram.org>. Without them social-cli
uses Telegram Desktop's public identity, as tg-cli does; Telegram tolerates
that, but its terms ask apps for their own. The login shows up in the app's
device list as "social-cli"; `logout` forgets it here — end it in the app.

## References

- Chats: `@name`, a `t.me/name` link, an invite link (`t.me/+…`), a
  private `t.me/c/<id>` link, or the numeric id `chats` shows (Bot API
  form: users positive, groups negative, channels `-100…`). `me` is your
  Saved Messages. Numeric ids work once the session has seen the chat
  (after `chats`).
- Messages: `t.me/name/42`, `t.me/c/<id>/42`, `t.me/name/42?comment=7`, or
  `CHAT/42` — the `id` every listed message carries.
- Forum topics: `chats GROUP` lists them; `messages GROUP/TOPIC` reads one.

## Chats and messages

- `chats` lists dialogs with unread counters; `messages CHAT [-n 500]
  [--since 7d]` reads history newest first (`-f jsonl` for exports).
- `send CHAT "text" [-a FILE …] [--reply-to MSG]`; several files go as an
  album. `edit`, `delete`, `react MSG 👍 [--undo]`, `pin`, `forward MSG CHAT`.
- `search WORDS [--filter photo|video|media|document|url|voice|music|gif|round|mention]`
  searches all chats; `search -t user NAME` finds people, groups and
  channels; `search-chat CHAT WORDS` one chat.
- `comments POST` reads a channel post's comments; `members CHAT` (admins
  only for bots); `follow CHAT` joins a channel or group (`--undo` leaves);
  `unread`; `mark-read CHAT`.
- `listen [CHAT …]` prints new, edited and deleted messages as they come
  (`kind`: `message`, `edit`, `delete`, `reactions`); without an account it
  checks public channels every 30 seconds.

## Analytics

Posts carry views, forwards (`shares`), comment counts and reactions
(`metrics.likes` is their total; `extra.reactions` lists them).

`insights @channel` shows the statistics Telegram keeps for channels and
supergroups you administer (Telegram enables them for larger ones):
followers / members with the previous period, views, shares and reactions
per post, notification rate, growth and interaction series, and breakdowns
by source, language and reaction. `insights CHANNEL/42` covers one post.

## Downloads

`download MSG` saves photos, videos, voice notes and documents in their
original quality. Bots fetch files up to 20 MB (a Bot API limit).

## Limits

Telegram answers floods with waits (`rate_limited`); short waits are
slept through. Bots cannot read history; the web preview shows the last
posts of public channels only.
