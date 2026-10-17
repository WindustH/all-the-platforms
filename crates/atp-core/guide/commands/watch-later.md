Your watch-later list (稍后再看, Watch later): `watch-later` lists it,
`later-add` and `later-remove` put posts on it and take them off, and
`later-clear` empties it. All need a login.

    atp bili later-add BV1xx411c7mD '#3'
    atp bili watch-later -n 50
    atp douyin later-remove '#2' '#4'
    atp douyin later-clear --watched
    atp bili later-clear -y

- `watch-later` gives Posts in the platform's order (usually the latest
  added first), and `#N` then refers to them. `-n` pages are followed
  automatically; `--cursor` continues later. `--since` / `--until` judge
  when a post was published, not when you added it, and a page of posts
  all published before `--since` ends the listing.
- POST: an id, link, `url` or `#N` of the last post list. `later-add` and
  `later-remove` take several and give an Action each, in order; one gives
  a single Action. A post that fails gets `ok: false` with `error` and
  `message`, and the rest go on; a login, a human check or a rate limit
  ends the run. The exit code is then 1.
- `later-clear` asks `empty your whole watch-later list? [y/N]` first
  when standard input is a terminal; scripts and MCP pass `-y`.
  `later-clear --watched` drops only the posts you already watched and
  asks nothing; it exists where the platform offers it
  (`later-clear-watched` in `atp platforms`).

More: `atp guide interact`, `atp guide refs`
