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
- `--all` replaces `-n` with every comment, however many pages that takes.
  `--max-pages N` bounds a run (each reply thread of `--replies` too): it
  stops after N upstream pages, says so on stderr and prints the cursor
  that continues.
- `--replies` completes the threads: where a comment carries fewer replies
  than its `reply_count`, all of them are fetched, three levels deep. It
  needs `replies` on the platform; elsewhere only the inline ones come. A
  thread cut short by `--max-pages` keeps its cursor in `replies_cursor`
  (`replies POST ID --cursor …` continues it).
- `--since` / `--until` filter by `created_at`, and a page entirely older
  than `--since` ends the listing. They apply to top-level comments, to
  `replies`, and with `--replies` to every reply in the threads.
- Output: a tree in the table, nested `replies` in JSON / YAML, and in JSON
  Lines / CSV one row per comment with `depth` (0 at the top) and
  `parent_id`. Fields: `id`, `author`, `text`, `created_at`, `likes`,
  `reply_count`, `reply_to` (whom it answers inside a thread),
  `location`, `replies_cursor`.
- JSON Lines and CSV keep rows only on stdout; where more remain, the
  cursor goes to stderr as `more: --cursor <c>`, as in the table.

Answer one with `comment POST TEXT --reply-to COMMENT`.

More: `atp guide analysis`, `atp guide refs`
