`comment` posts a comment on a post, or with `--reply-to` a reply to one of
its comments. `delete-comment` removes one of your comments. Both need a
login.

    atp bili comment BV1xx411c7mD '好耶'
    atp bili comment '#1' '同意' --reply-to 2384729384
    cat reply.md | atp zhihu comment '#2' -
    atp x comment '#1' 'Numbers for this week' -i chart.png
    atp bili delete-comment BV1xx411c7mD 2384729384 -y

- POST: an id, link, `url` or `#N` of the last post list. COMMENT: the id
  `comments` prints, as given (comments have no `#N`); some platforms
  take more forms (Bilibili `ROOT:PARENT`, links on Reddit and X).
- TEXT `-` reads the text from standard input (not over MCP).
- `-i IMAGE` (repeatable) attaches images where the platform takes them in
  comments (`comment-images` in `atp platforms`; the option is hidden
  elsewhere). The text may then be left out. A missing file stops the
  command before anything is sent.
- The result is an Action with the new comment's `id` and `url` when the
  platform returns them; that id works for `--reply-to` and
  `delete-comment`.
- `delete-comment` asks `delete comment ID? [y/N]` when standard input is
  a terminal. Scripts, piped input and MCP must pass `-y` / `--yes`;
  otherwise it is refused and nothing is deleted.

More: `atp guide interact`
