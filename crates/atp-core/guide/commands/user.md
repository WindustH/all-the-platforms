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
- `following --sort KEY` orders the list where the platform can:
  `recent` (latest follow first), `oldest`, `smart` (the platform's own
  mix), `visited` (the ones you visit most); `atp guide <platform>
  following` shows the platform's values. `--search WORDS` keeps the
  accounts whose name matches, `--mutual` the ones that follow you back.
  Where a platform lacks one of them it is hidden and refused.
- On your own lists, users carry what the platform tells about the
  relation in `extra`: `mutual` (true when you follow each other),
  `followed_at` (when the follow began), `groups` (the ids of the follow
  groups they are in, `[]` for none; see `follow-groups`), `special` (a
  special follow, 特别关注) and `remark` (the name you gave them). Keys a
  platform does not know are left out.
- `#N` afterwards refers to the posts or users just listed.

Act on the lists with `follow` / `unfollow`, `remove-follower`, `block`
and the follow-group commands (`follow-groups`, `follow-group-add` ...).

More: `atp guide refs`, `atp guide analysis`
