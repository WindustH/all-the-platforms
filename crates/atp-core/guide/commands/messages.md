Lists the messages of one chat, newest first, paging back through its
history as far as `-n` asks.

    atp tg messages @rustlang -n 200
    atp discord messages '#3' -n 1000 --since 7d -f jsonl > channel.jsonl

- CHAT: the chat's id or link as the platform shows it, or `#N` after
  `chats`; the platform's notes list further forms.
- A message is a Post: `id`, `kind`, `text`, `author`, `created_at`,
  `media` (attachments), `quoted` (a quoted or answered message, where
  given), platform fields in `extra`. A message without a link of its own
  has `CHAT/MESSAGE` as its `id`.
- `#N` afterwards refers to these messages, in `read`, `react`, `edit`,
  `delete`, `download` and `send --reply-to`.
- `--since 7d` stops at the first page older than a week, so only what is
  needed is read. `--until` alone skips newer pages; after three in a row
  with nothing inside the window it stops and prints a cursor to go on.
- `--cursor` continues an export later. JSON Lines and CSV print rows
  only, without the cursor; use `--json` when you will need it.

New messages as they arrive: `listen`.

More: `atp guide refs`, `atp guide output`
