Searches the platform the way its own search box does: posts by default,
accounts with `-t user`, topics with `-t topic`.

    atp bili search "rust 教程" -n 10
    atp bili search 影视飓风 -t user
    atp x search "AI agent" --sort latest --since 1d -f csv > tweets.csv

- `-t post` lists Posts: videos, notes, tweets, answers, products, places,
  songs ... (`kind` says which). `-t user` lists Users, `-t topic`
  Collections (topics, hashtags, subreddits ...). A type the platform
  cannot search answers `unsupported_operation`.
- `--sort` and `--filter` take this platform's own values, listed above;
  the platform's notes say what they mean.
- `-n` results are gathered across as many pages as needed; a search that
  stops early prints a cursor for `--cursor`. Results the platform repeats
  on later pages are dropped.
- `--since` / `--until` apply to post results only: posts outside the
  window are dropped, and a page made entirely of posts older than
  `--since` ends the search (right for newest-first orders). User and topic
  searches refuse them.
- Afterwards `#N` means the N-th result. Posts, users and topics keep
  separate lists, so `atp xhs read '#2'` takes the second post and
  `atp bili user-posts '#1'` the first user.

### Cloud drives

On a drive `search` matches the names of your files and folders and lists
File entries (`id`, `name`, `path`, `dir`, `size`, `updated_at`); `#N`
then works in `read`, `download` and `delete`. `-t` does not apply, and
neither do `--since` / `--until`.

More: `atp guide refs`, `atp guide output`
