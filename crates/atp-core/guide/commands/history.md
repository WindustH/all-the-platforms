Lists what the logged-in account viewed or read, as the platform records
it. It needs a login.

    atp bili history -n 50
    atp bili history -n 500 -f csv > watched.csv

- The result is Posts in the platform's order (usually the latest visit
  first); what the platform knows about the visit goes into `extra`
  (Bilibili: `viewed_at`, `progress`). `#N` then refers to them.
- `-n` pages are followed automatically; `--cursor` continues later.
- `--since` / `--until` judge when a post was published (`created_at`),
  not when you viewed it, and a page of posts all published before
  `--since` ends the listing. To cut by viewing time, export and filter on
  the platform's field in `extra`.
- Searching, pausing or clearing the history are platform commands where
  they exist (see the platform's notes).

More: `atp guide refs`
