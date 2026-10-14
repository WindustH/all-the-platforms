WeChat's own **ClawBot** (微信 ClawBot, the iLink bot API that Tencent's
`openclaw-weixin` plugin uses). Logging in attaches a bot to your WeChat
account: a **ClawBot** contact then appears in WeChat, people message it,
and `atp wechat` receives those messages and answers them. It is not
automation of your WeChat account.

## Logging in

`atp wechat login` shows a QR code. Scan it with WeChat on the phone and
confirm. Along the way:

- WeChat may ask you to type the number shown on the phone. The login asks
  for it in the terminal (so it must run in one); after wrong numbers
  WeChat blocks the login for a while ("try again later").
- A code that expires is replaced by a new one, up to three times.
- If the WeChat account is already connected to a ClawBot client, the login
  fails ("already connected"): disconnect it in WeChat (the ClawBot
  contact's settings) and log in again. When that connection is the one
  saved here, the saved login is simply kept.

The session holds the bot's token, its id, the owner's id (the WeChat user
who scanned) and the API host WeChat assigned. How long the token lasts is
not known. An expired one answers `errcode -14`: commands fail with
`not_authenticated` ("the WeChat bot session expired"), `listen` stops,
and `atp wechat login` starts a new session. `logout` forgets the token on
this machine only; disconnect the bot in WeChat to end it there.

`status` and `whoami` check the token with a short poll, so they also
receive (and record) messages that were waiting.

There is no token or cookie login, and nothing works without logging in.
Tencent presents ClawBot as being for personal use and private chats. Use
it within WeChat's terms of service.

## References

- Chats: the other person's id, as `chats` lists it (`…@im.wechat`), or `#N`.
- Messages: `CHAT/MESSAGE`, or `#N` after `messages`.

## Commands

### chats

The conversations this machine has seen, newest first. `can_reply` says
whether a context token is on record, which answers need.

### messages, read

The recorded messages of a chat, newest first, and one of them.

Items: text, images, voice (with WeChat's transcription when present),
files and videos. Quoted messages appear in `quoted`.

### download

Saves the images, videos, files and voice notes (`.silk`) of a recorded
message, decrypted.

### send

`send CHAT "text" [-a FILE …] [--reply-to CHAT/MESSAGE]`: the text first,
then each file as its own message. Images (jpg/png/gif/webp) and videos
(mp4/mov) are sent as such, anything else as a file. Files are uploaded
AES-encrypted to WeChat's CDN, as the official client does.

The bot cannot message people out of the blue: it answers conversations
people start with it, and WeChat ties every answer to the latest
`context_token` of that conversation. Without one on record ATP still
sends, and warns that WeChat may not deliver it until they message the bot.

`--reply-to` answers with that message's context token (it must be a
message of the same chat). WeChat shows no quote.

### listen

`listen [CHAT…]` streams messages as they arrive (`-f jsonl` for
scripts). Failed polls are retried after a pause; an expired login ends
it.

### sync

`sync [--wait SECS]` fetches what is queued once and records it.

### typing

`typing CHAT [--stop]` shows "typing…" in a chat that messaged the bot
recently.

## History lives on this computer

There is no message history on the server. A message is delivered once,
to whichever client polls first (`listen`, `sync`, also `status`).

Each message is recorded here, in the private cache: the update cursor, the
latest `context_token` of each conversation, and the last 500 messages of
each chat (what `send` sent included). `chats`, `messages`, `read`,
`download` and replies work from that record, so they know only what this
machine received.

## Limits

- It cannot read your chats, contacts, groups or Moments.
- No chat listing or history beyond what this machine recorded. No groups,
  reactions, edits, deletes or member lists.
- ATP keeps at least 300 ms between requests.
