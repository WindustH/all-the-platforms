`likers` lists the accounts that liked a post; `reposts` lists the posts
that repost, retweet, quote or crosspost it. Some platforms show these only
for certain posts, such as your own; the platform's notes say which.

    atp zhihu likers '#1' -n 100
    atp x reposts 2104695667180380260 -n 50 -f csv > reposts.csv

- POST: an id, link, `url` or `#N` of the last post list.
- `likers` gives Users (`id`, `name`, `handle`, `url`, `stats`,
  `followed` ...), and `#N` then refers to them: `atp x user '#3'`. It
  takes no `--since` / `--until`.
- `reposts` gives Posts with their own text, author and counters; `#N`
  then refers to them. `--since` / `--until` keep those published in the
  window; items without a date are kept.
- `-n` and `--cursor` page as everywhere; repeated items are dropped.

More: `atp guide analysis`, `atp guide refs`
