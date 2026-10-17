Changes one of your messages, or one of your published posts where the
platform lets posts be edited. On chat platforms it replaces a message's
text; on content platforms it can also change a post's title, topics,
cover and visibility.

    atp tg edit '#1' 'Fixed the link: https://example.com'
    cat fixed.md | atp discord edit 123456/789012 -
    atp douyin edit 7697611474302225714 --topic 操作系统 --topic 计算机
    atp douyin edit '#2' --title '新标题' --visibility private

- POST: `CHAT/MESSAGE`, an id, a link, or `#N` of the last message or
  post list; the `id` that `send` or `post` printed works too.
- TEXT replaces the whole text (a message, a description, a body); `-`
  reads it from standard input (not over MCP). An empty text is refused
  where the text is all that changes.
- Where the platform offers them (its capabilities in `atp platforms`):
  `--title` (`edit-title`); `--topic NAME` (repeatable, `edit-topics`)
  replaces the post's topics or hashtags and `--no-topics` removes them;
  `--cover PATH` (`edit-cover`); `--visibility public|friends|private`
  (`edit-visibility`). Elsewhere they are hidden and refused.
- What is not given stays as it was.
- Platforms that show the change before sending it (Douyin) ask first;
  scripts pass `-y`.
- The result is an Action with the post's `id` and `url`.

More: `atp guide refs`
