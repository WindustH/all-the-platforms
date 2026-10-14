Replaces the text of one of your messages, or of a post where the platform
lets posts be edited.

    atp tg edit '#1' 'Fixed the link: https://example.com'
    cat fixed.md | atp discord edit 123456/789012 -

- POST: `CHAT/MESSAGE`, a link, or `#N` of the last message or post list;
  the `id` that `send` printed works too.
- TEXT replaces the whole text; `-` reads it from standard input (not over
  MCP). An empty text is refused.
- The result is an Action.

More: `atp guide refs`
