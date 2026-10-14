`hot` lists what is trending on the platform now, as its trending, popular
or ranking pages show it. `feed` lists the account's home timeline or
recommendations, as the app's home page does. Both print Posts.

    atp bili hot -n 5
    atp x hot --category news
    atp x feed --type following -n 50 -f jsonl > timeline.jsonl

- `hot --category` and `feed --type` pick one of the platform's lists
  (sections, regions, following or recommended ...); the values are listed
  above. Without them the platform's default list comes.
- `feed` follows your account, so platforms that personalise it need a
  login (`not_authenticated` otherwise).
- `-n` pages are followed until enough posts arrive; `--cursor` continues
  later, and posts repeated across pages are dropped.
- `--since` / `--until` keep posts published inside the window. Trending
  lists are ranked rather than ordered by date, so the window only filters,
  but a page made entirely of posts older than `--since` still ends the
  listing.
- `#N` afterwards refers to these posts: `atp bili read '#1'`.

Saving `hot` at intervals builds a trend history the platforms do not keep
(`atp guide analysis`).

More: `atp guide refs`
