Sends a message to a chat as your account or bot.

    atp tg send @mygroup 'Deploy finished'
    atp tg send '#1' 'Photos from today' -a a.jpg -a b.jpg
    atp discord send '#3' 'Agreed' --reply-to 123456/789012
    echo "build ok" | atp slack send C0123456 -

- CHAT: the chat's id or link, or `#N` after `chats`; the platform's
  notes list further forms (some platforms take a user for the direct chat
  with them).
- TEXT `-` reads the text from standard input (not over MCP). It may be
  left out when files are attached.
- `-a PATH` (repeatable) attaches files: images, videos, documents. Each
  must exist; a missing one stops the command before anything is sent.
- `--reply-to MESSAGE` answers a message: `CHAT/MESSAGE`, a link, or `#N`
  after `messages`.
- The result is an Action with the sent message's `id` and `url` when the
  platform returns them; they work in `edit`, `react` and `delete`.
- Nothing asks for confirmation. Platforms restrict accounts that send in
  loops; keep automated sending light.

More: `atp guide refs`
