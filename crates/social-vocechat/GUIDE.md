## Servers and accounts

VoceChat is self-hosted: every command talks to the server you logged in to.

- `social vocechat login --token 'SERVER|EMAIL|PASSWORD'` logs in as a user
  (pipe it through `--token -` to keep the password out of the shell
  history). The access token expires within minutes and is renewed with the
  refresh token as needed.
- `social vocechat login --token 'SERVER|BOT_API_KEY'` uses a bot (Settings
  → Bot & Webhook). Bots can send to users and to the channels they are in,
  list those channels and look up users; no history, edits, reactions or
  live events (VoceChat delivers those to bots by webhook).
- `VOCECHAT_URL` + `VOCECHAT_API_KEY` supply a bot for one run.

## References

- Chats: `group:<gid>` (a channel), `user:<uid>` (a direct chat), or the web
  app's `.../#/chat/channel/<gid>` / `#/chat/dm/<uid>` links. `#N` works
  after `chats`.
- Messages: `group:<gid>/<mid>` / `user:<uid>/<mid>` (the `id` of listed
  messages).
- Users: `user:<uid>`, the number, or `me`.

## Commands

- `chats` (your channels, then a direct chat with every user),
  `messages CHAT [-n N] [--since 7d]` (edits, deletions and reactions are
  folded into the messages they touch), `read`, `members`, `user`.
- `send CHAT TEXT [-a FILE ...] [--reply-to MESSAGE]`, `markdown CHAT TEXT`,
  `edit`, `delete`, `react MESSAGE 👍` (VoceChat takes ❤️ 😄 👀 👍 👎 🎉 🙁 🚀;
  the same reaction again takes it back), `download MESSAGE`.
- `listen [CHAT ...]` follows the user event stream.

## Limits

No search (VoceChat has no search API) and no pinning yet.
