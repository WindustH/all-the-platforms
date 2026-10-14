`notifications` lists what the platform told the account: likes, comments,
replies, mentions, new followers, system messages. `unread` prints the
unread counters. Both need a login.

    atp zhihu notifications -n 50
    atp bili notifications --type reply --since 7d
    atp bili unread

- `--type` picks one kind where the platform keeps them apart; the values
  are listed above.
- `notifications` gives Notifications: `id`, `kind` (`like`, `comment`,
  `mention`, `follow`, `reply`, `system` ...), `text`, `actor` (a User),
  `target` (title or excerpt of the content concerned), `url`,
  `created_at`, `unread`.
- They get no `#N`; pass a notification's `url` to `read` or `comments`
  where it links a post.
- `-n`, `--cursor` and `--since` / `--until` work as on other dated lists:
  a page entirely older than `--since` ends the listing.
- `unread` gives named counters (the names are the platform's): a line each
  in the table, one object in JSON / YAML, one row in CSV.

More: `atp guide refs`
