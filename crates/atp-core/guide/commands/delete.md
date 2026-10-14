Deletes one of your posts. On chat platforms it deletes a message; on a
drive it moves a file or folder to the recycle bin.

    atp x delete 2104695667180380260
    atp bili delete '#1' -y
    atp discord delete 123456/789012 --yes
    atp quark delete /Old/report.pdf --yes

- POST: an id, link, `url` or `#N` of the last post list; the `id` or
  `url` that `post` or `send` printed works too. A message is
  `CHAT/MESSAGE` or a link; on a drive a path, an id or `#N` of a listing.
- It asks `delete …? [y/N]` first when standard input is a terminal.
  Scripts, cron jobs, piped input and MCP must pass `-y` / `--yes`;
  without it the command is refused (`invalid_input`) and nothing is
  deleted.
- The result is an Action (`action`, `target`, `ok`).

Comments go with `delete-comment POST COMMENT`.

More: `atp guide interact`
