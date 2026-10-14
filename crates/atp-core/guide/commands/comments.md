`comments` lists the comments of a post, each with the replies the platform
shows inline. `replies` lists the replies under one comment.

    atp bili comments BV1xx411c7mD -n 50
    atp bili replies BV1xx411c7mD 2384729384
    atp reddit comments 1wsxldl --all --replies -f jsonl > thread.jsonl

- POST: an id, link, `url` or `#N` of the last post list. COMMENT: the id
  `comments` prints (`id …` in the table, `id` in data). Comments get no
  `#N`; the numbers in the table are positions only.
- `-n` counts top-level comments; inline replies come along uncounted.
  `--sort` takes the platform's values, listed above.
- `--all` replaces `-n` with every comment, up to 200 upstream pages in one
  run; a cursor continues when more remain.
- `--replies` completes the threads: where a comment carries fewer replies
  than its `reply_count`, all of them are fetched, three levels deep. It
  needs `replies` on the platform; elsewhere only the inline ones come.
- `--since` / `--until` filter by `created_at`, and a page entirely older
  than `--since` ends the listing. They apply to top-level comments and to
  `replies`; the replies `--replies` fetches are not filtered.
- Output: a tree in the table, nested `replies` in JSON / YAML, and in JSON
  Lines / CSV one row per comment with `depth` (0 at the top) and
  `parent_id`. Fields: `id`, `author`, `text`, `created_at`, `likes`,
  `reply_count`, `reply_to` (whom it answers inside a thread),
  `location`.
- JSON Lines and CSV print rows only, without the cursor; use `--json` to
  continue a long export later.

Answer one with `comment POST TEXT --reply-to COMMENT`.

More: `atp guide analysis`, `atp guide refs`
