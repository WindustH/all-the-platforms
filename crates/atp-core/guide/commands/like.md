Act as your account: `like` / `unlike` posts, `favorite` / `unfavorite`
(save or bookmark) them, `follow` / `unfollow` users. All need a login.

    atp bili like BV1xx411c7mD
    atp x unlike '#3'
    atp bili favorite '#1' --folder 12345678
    atp douyin unfavorite '#2' '#5' '#8'
    atp x follow NASA
    atp bili follow 946974 '#2' '#5'

- `unlike`, `unfavorite` and `unfollow` are the same as `like --undo`,
  `favorite --undo` and `follow --undo`.
- POST: an id, link, `url` or `#N` of the last post list. USER: an id,
  handle, link, `url` or `#N` of the last user list.
- `--folder F` saves into (or removes from) one folder or list: its id, or
  `#N` after `collections`. Without it each platform uses its default.
- Each command takes several posts or users. One gives an Action, as
  always; several give one Action each, in order, through the platform's
  batch call where it has one, else one by one. One that fails gets
  `ok: false` with `error` and `message` and the rest go on; a login, a
  human check or a rate limit ends the run and marks the rest as not
  tried. The exit code is 1 when any failed. `--folder` applies to every
  post given.
- Some platforms give `follow` a wider meaning, such as joining a channel
  or group, or subscribing to a community; the platform's notes say what
  USER may be there.
- The result is an Action: `action`, `target`, `ok`, and a `message` when
  the platform adds one. Nothing asks for confirmation.
- Platforms restrict accounts that like or follow in loops: space bulk
  work out with `--interval`, and stop at `rate_limited` or
  `verification_required`.

More: `atp guide interact`
