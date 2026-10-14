## Logging in

There are two ways in: a personal account as a linked device, or the
Business Cloud API. When both are set up, the linked device is used.
Linking a device drops saved Cloud API credentials, and `login --token`
replaces a linked device.

### Personal account (linked device)

`atp whatsapp login` shows a QR code: on the phone open WhatsApp → Settings
→ Linked devices → Link a device and scan it. When a code expires a new one
is drawn (with the path of its image); when WhatsApp stops sending codes the
login ends as expired.

Without a camera at hand, `atp whatsapp login-code +NUMBER` (the account's
number with its country code) prints a code to type on the phone under
Linked devices → Link a device → Link with phone number instead. It waits
up to four minutes for the code to be entered.

Once linked, ATP receives the phone's recent chats for up to two minutes
(see "History lives on this computer") and then works like WhatsApp Web:
chats, groups, messages, sending, reactions, edits, deletions and live
events. The device's keys are kept in a private file next to the session
(`~/.config/all-the-platforms/whatsapp/device.json`).

The link lasts until it is removed; how long WhatsApp keeps an idle linked
device is not known. When the device is removed on the phone (Linked
devices), commands fail with `not_authenticated` ("WhatsApp logged this
device out"): link it again with `atp whatsapp login`. `atp whatsapp unlink`
removes the linked device on WhatsApp and deletes its keys and archive;
`logout` only forgets it here.

This is an unofficial client of WhatsApp's protocol; automating a personal
account is against WhatsApp's terms and can get it banned, so keep volumes
human. A temporary ban shows as `ip_blocked` ("WhatsApp temporarily banned
this account"). Linking also fails when the phone's WhatsApp is too old for
linked devices, or when WhatsApp rejects this client version (update ATP).

### Business Cloud API

    atp whatsapp login --token 'TOKEN:PHONE_NUMBER_ID[:WABA_ID]'

- `TOKEN`: a system-user access token from Meta Business Manager, with the
  `whatsapp_business_messaging` and `whatsapp_business_management`
  permissions.
- `PHONE_NUMBER_ID`: the id (digits) of the business phone number, from
  Meta's WhatsApp Manager or App Dashboard.
- `WABA_ID`: the WhatsApp Business Account id (digits); `numbers`,
  `templates` and `insights` need it.

The parts may also be separated by `|`. `login` keeps them and checks them
with `whoami` (the number's verified name, quality rating, messaging limit
...). The Cloud API sends text, media and templates, reacts, marks read,
downloads media by id and reports messaging analytics. It cannot list
chats, read history or `listen`: incoming messages go to a webhook. The
commands of the other mode fail with `unsupported_operation`.

How long the token lasts is not known to ATP. When it expires or is
invalid, commands fail with `not_authenticated` (Cloud API error 190):
create a new system-user token and log in again. Missing permissions give
`permission_denied`.

Environment variables (never written):

- `WHATSAPP_TOKEN` + `WHATSAPP_PHONE_NUMBER_ID`, optionally
  `WHATSAPP_BUSINESS_ACCOUNT_ID`: Cloud API credentials, used when none are
  saved and no device is linked.
- `WHATSAPP_GRAPH_VERSION`: the Graph API version (default `v23.0`).

## References

- Chats: a phone number with its country code (`+44 7700 900123`), a JID
  (`447700900123@s.whatsapp.net`, `…@g.us` for groups), a name the archive
  knows (it must match one chat), `me`, or `#N` after `chats`.
- Messages: `CHAT/ID` as `messages` prints them, or `#N`. With the Cloud
  API, `TO/wamid.…` as `send` prints it.
- Cloud API recipients: phone numbers with their country code.

## Commands

The linked device runs every command except `profile`, `numbers`,
`templates`, `template` and `insights`, which are Cloud API only. The Cloud
API runs only those five plus `send`, `react`, `mark-read`, `download` and
`unlink`.

### chats

From the archive; group names come from the server.

### messages

Newest first, from the archive. Paging past the oldest archived message
asks the phone for earlier messages; it answers while it is online.

### read

One message from the archive.

### search

`search WORDS` finds messages in the archive whose text contains the words
(ignoring case).

### unread

Unread counts per chat from the archive, and the total.

### members

The members of a group.

### user

`user NUMBER|GROUP`: a contact's name, status text, picture and business
flag (numbers not on WhatsApp are `not_found`), or a group's subject,
description and member count.

### send

`send CHAT TEXT [-a FILE …] [--reply-to MESSAGE]`: images, videos and audio
are sent as such, other files as documents; the text is the first file's
caption. With the Cloud API, audio and stickers take no caption: the text
goes with the first file that does, or as its own message.

### react

`react MESSAGE EMOJI [--undo]`. With the Cloud API, MESSAGE is `TO/WAMID`
as `send` printed it.

### edit

Only your own messages.

### delete

Deletes for everyone: your messages, or members' messages in groups you
admin.

### follow, unfollow

`follow INVITE_LINK` joins a group by its `chat.whatsapp.com` invite link;
`unfollow GROUP` (or `follow GROUP --undo`) leaves it.

### listen

`listen [CHAT …]`: new messages, edits, deletions and reactions as they
arrive (`-f jsonl` for one JSON object per event); Ctrl-C stops.

### download

Personal: the message's photo, video, audio or document. Media of old
messages expire on WhatsApp's servers. Cloud API: `download MEDIA_ID`, with
a media id from a webhook payload (Cloud API media live 30 days).

### older

`older CHAT [-n N]` asks the phone for up to N (1 to 500) messages older
than the oldest archived one; the phone must be online, and at least one
message of the chat must be archived.

### mark-read

Personal: `mark-read CHAT` sends read receipts for the chat's unread
messages. Cloud API: `mark-read TO/WAMID` marks that message (and those
before it) as read.

### groups

Your groups, from the server rather than the archive.

### check

`check NUMBER …`: which phone numbers are on WhatsApp.

### login-code

Links with a code typed on the phone; see "Logging in".

### unlink

Removes the linked device on WhatsApp and deletes its keys and archive
(`logout` only forgets it here). With Cloud API credentials, it forgets
them.

### profile, numbers, templates

Cloud API: the business profile, the business account's phone numbers and
its message templates. `numbers` and `templates` need the WABA id.

### template

`template TO NAME [--lang en_US] [-p PARAM …]` sends an approved template.
Outside the 24-hour customer-service window only templates reach people.

### insights

Cloud API, with the WABA id: messages sent and delivered per day,
conversation volume and cost by pricing category, for up to 90 days.

## History lives on this computer

WhatsApp keeps no message history on its servers. Right after linking, the
phone sends its recent chats; after that, every command catches up on what
arrived since the last one, and `listen` receives messages live. All of it
is kept in a private archive next to the session
(`~/.config/all-the-platforms/whatsapp/archive/`), which `chats`,
`messages`, `read` and `search` read. Paging past the oldest archived
message (or `older CHAT -n 100`) asks the phone for earlier messages; it
answers while it is online.

## Limits

- One linked-device connection at a time: a running `listen` and another
  command take turns. When another connection takes over while a command
  connects, it fails with "another connection of this linked device took
  over".
- Where WhatsApp is blocked, the linked device connects through `--proxy`
  or the proxy variables (`ATP_PROXY`, `ALL_PROXY`, `HTTPS_PROXY`): HTTP
  proxies by `CONNECT`, or SOCKS5.
- Cloud API requests are spaced at least 300 ms apart; its rate limits
  show as `rate_limited`.
