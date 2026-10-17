Lists what the logged-in account viewed or read, as the platform records
it; `history-delete` removes single posts from it and `history-clear`
erases all of it. They need a login.

    atp bili history -n 50
    atp bili history -n 500 -f csv > watched.csv
    atp bili history-delete '#2' '#5'
    atp xhs history-clear -y

- The result is Posts in the platform's order (usually the latest visit
  first); what the platform knows about the visit goes into `extra`
  (Bilibili: `viewed_at`, `progress`). `#N` then refers to them.
- `-n` pages are followed automatically; `--cursor` continues later.
- `--since` / `--until` judge when a post was published (`created_at`),
  not when you viewed it, and a page of posts all published before
  `--since` ends the listing. To cut by viewing time, export and filter on
  the platform's field in `extra`.
- `history-delete POST…` takes ids, links or `#N` of the last post list
  (after `history`, the entries themselves). One post gives an Action;
  several give one Action each, in order. A post that fails gets `ok:
  false` with `error` and `message`, and the rest go on; a login, a human
  check or a rate limit ends the run. The exit code is then 1.
- `history-delete` and `history-clear` ask first (`remove … from your
  history? [y/N]`, `erase your whole history? [y/N]`) when standard input
  is a terminal; scripts and MCP pass `-y`. Neither can be undone.
- Platforms differ in what they offer: some only clear the whole history,
  and searching, pausing or counting the history are platform commands
  where they exist (see the platform's notes).

More: `atp guide refs`
