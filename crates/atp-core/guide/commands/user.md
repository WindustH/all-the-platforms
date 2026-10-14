`user` shows one profile. `user-posts` lists what a user published,
`followers` who follows them and `following` whom they follow.

    atp x user NASA
    atp bili user-posts 946974 -n 100 --since 90d -f csv > videos.csv
    atp xhs user-posts '#1'           # after a user list
    atp bili followers -n 50          # your own followers

- USER takes what the platform shows: an id, a handle (or a name where the
  platform looks names up), a profile link, the `url` of an earlier
  result, or `#N` of the last user list (from `search -t user`,
  `followers`, `following`, `likers`, `members` ...).
- Without USER, `user-posts`, `followers` and `following` use your own
  account, looked up with `whoami` first, so they need a login.
- `user` gives a User: `id`, `name`, `handle`, `url`, `avatar`, `bio`,
  `verified`, `location`, `stats` (`followers`, `following`, `posts`,
  `likes` and platform counters), `followed` (whether you follow them,
  when known), `created_at`.
- `user-posts` gives Posts and takes `--since` / `--until`: posts outside
  the window are dropped, and a page entirely older than `--since` ends
  the listing. With `--until` alone, newer pages are skipped; after three
  pages in a row with nothing inside the window the listing stops and
  prints a cursor to go on.
- `followers` and `following` give Users and refuse `--since` / `--until`.
- `#N` afterwards refers to the posts or users just listed.

More: `atp guide refs`, `atp guide analysis`
