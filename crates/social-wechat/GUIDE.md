## What this is

WeChat's own **ClawBot** (微信 ClawBot, the iLink bot API that Tencent's
`openclaw-weixin` plugin uses). Logging in attaches a bot to your WeChat
account. A **ClawBot** contact then appears in WeChat, people message it,
and `social wechat` receives those messages and answers them.

It is not automation of your WeChat account:

- It cannot read your chats, contacts, groups or Moments.
- It cannot message people out of the blue: it answers conversations
  people start with the bot. WeChat ties every answer to the latest
  `context_token` of that conversation.
- There is no message history on the server. A message is delivered once,
  to whichever client polls first (`listen`, `sync`, also `status`).
- Tencent presents ClawBot as being for personal use and private chats.
  Use it within WeChat's terms of service.

## Logging in

`social wechat login` shows a QR code. Scan it with WeChat on the phone and
confirm. WeChat may ask you to type the number shown on the phone.

The session holds the bot's token, which expires. An expired token answers
`errcode -14`; log in again. `logout` forgets the token on this machine
only; disconnect the bot in WeChat to end it there.

## References

- Chats: the other person's id, as `chats` lists it (`…@im.wechat`), or `#N`.
- Messages: `CHAT/MESSAGE`, or `#N` after `messages`.

## Receiving

- `listen [CHAT…]` streams messages as they arrive (`-f jsonl` for scripts).
- `sync [--wait SECS]` fetches what is queued once.

Each message arrives once. It is recorded here (in the private cache, with
the conversation's `context_token`). `chats` lists the conversations seen,
newest first; `can_reply` says whether a context token is on record.
`messages CHAT` and `read` show the recorded messages, and `download`
saves their images, videos, files and voice notes.

Items: text, images, voice (with WeChat's transcription when present,
saved as `.silk`), files and videos. Quoted messages appear in `quoted`.

## Sending

`send CHAT "text" [-a FILE …] [--reply-to CHAT/MESSAGE]`: the text first,
then each file as its own message. Images (jpg/png/gif/webp) and videos
(mp4/mov) are sent as such, anything else as a file. Files are uploaded
AES-encrypted to WeChat's CDN, as the official client does.

`--reply-to` answers with that message's context token. WeChat shows no
quote.

`typing CHAT [--stop]` shows "typing…" in a chat that messaged the bot recently.

## Limits

No chat listing or history beyond what this machine recorded. No groups,
reactions, edits, deletes or member lists.
