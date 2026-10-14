Prints messages and other events the moment they arrive, until you
interrupt it (Ctrl-C) or the connection ends.

    atp tg listen                     # everything the account receives
    atp discord listen '#3' -f jsonl  # one channel, a JSON object per event
    atp bili listen live:ROOM         # danmaku of a live room

- CHAT (repeatable) keeps to those chats: an id, a link or `#N` after
  `chats`. Without one, everything the account receives.
- Each event is a Post, printed at once: a line in the table (time, chat,
  author, text); one JSON object per line with `jsonl`, `json` and `csv`;
  one YAML document with `yaml`. `platform` and `fetched_at` are added.
- Events other than new messages (edits, deletions, reactions ...) carry
  their kind in `kind`, such as `edit` or `delete`.
- How events arrive (a live connection or polling) and what a bot receives
  depend on the platform; see the platform's notes.
- Over MCP, `listen` returns the events seen within `wait_secs` (default
  30 s) as a list of posts.

More: `atp guide output`, `atp guide mcp`
