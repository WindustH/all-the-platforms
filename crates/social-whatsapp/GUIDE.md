## Two ways in

- **Personal account (linked device).** `social whatsapp login` shows a QR
  code: on the phone open WhatsApp → Settings → Linked devices → Link a
  device and scan it. Without a camera at hand, `login-code +NUMBER` prints
  a code to type under "Link with phone number instead". social-cli then
  works like WhatsApp Web: chats, groups, messages, sending, reactions,
  edits, deletions and live events. This is an unofficial client of
  WhatsApp's protocol; automating a personal account is against WhatsApp's
  terms and can get it banned, so keep volumes human.
- **Business Cloud API.** `social whatsapp login --token
  TOKEN:PHONE_NUMBER_ID[:WABA_ID]` with a system-user access token from
  Meta Business Manager (or `WHATSAPP_TOKEN`, `WHATSAPP_PHONE_NUMBER_ID`,
  `WHATSAPP_BUSINESS_ACCOUNT_ID`). It sends text, media and templates,
  reacts, marks read, downloads media by id and reports messaging
  analytics. It cannot list chats or read history: incoming messages go to
  a webhook.

## History lives on this computer

WhatsApp keeps no message history on its servers. Right after linking, the
phone sends its recent chats; after that, every command catches up on what
arrived since the last one, and `listen` receives messages live. All of it
is kept in a private archive next to the session
(`~/.config/social-cli/whatsapp/archive/`), which `chats`, `messages`,
`read` and `search` read. Paging past the oldest archived message (or
`older CHAT -n 100`) asks the phone for earlier messages; it answers while
it is online.

## References

- Chats: a phone number with its country code (`+44 7700 900123`), a JID
  (`447700900123@s.whatsapp.net`, `…@g.us` for groups), a name the archive
  knows, `me`, or `#N` after `chats`.
- Messages: `CHAT/ID` as `messages` prints them, or `#N`. With the Cloud
  API, `TO/wamid.…` as `send` prints it.

## Commands

- `chats`, `messages CHAT`, `read`, `search WORDS` (in the archive),
  `unread`, `members GROUP`, `user NUMBER|GROUP`.
- `send CHAT TEXT [-a FILE …] [--reply-to MESSAGE]`: images, videos and
  audio are sent as such, other files as documents; the text is the first
  file's caption.
- `react MESSAGE EMOJI [--undo]`, `edit MESSAGE TEXT`, `delete MESSAGE`
  (for everyone: your messages, or members' messages in groups you admin).
- `follow INVITE_LINK` joins a group; `follow GROUP --undo` leaves it.
- `listen [CHAT …]`: new messages, edits, deletions and reactions as they
  arrive (`-f jsonl` for one JSON object per event); Ctrl-C stops.
- `download MESSAGE`: the message's photo, video, audio or document.
- Personal only: `older CHAT [-n N]`, `mark-read CHAT`, `groups`,
  `check NUMBER …` (who is on WhatsApp), `login-code PHONE`, `unlink`
  (removes the linked device and deletes its keys and archive; `logout`
  only forgets it here).
- Cloud API only: `profile`, `numbers`, `templates`, `template TO NAME
  [--lang en_US] [-p PARAM …]` (outside the 24-hour customer-service window
  only templates reach people), `mark-read TO/WAMID`, `download MEDIA_ID`,
  `insights [--days N]` (messages sent and delivered per day, conversation
  volume and cost by pricing category; needs the WABA id).

## Limits

- One linked-device connection at a time: a running `listen` and another
  command take turns.
- Media of old messages expire on WhatsApp's servers.
- `--proxy` does not apply to the personal account's connection.
