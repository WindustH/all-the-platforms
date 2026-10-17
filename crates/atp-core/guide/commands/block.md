`block` stops users from reaching your account and `unblock` lets them
back; `blocked` lists the accounts you block. `remove-follower` makes
users stop following you without blocking them. All need a login.

    atp bili block 946974
    atp bili block '#2' '#4' '#9'
    atp bili unblock 946974
    atp bili blocked -n 100 -f csv > blocked.csv
    atp bili remove-follower '#3' -y

- USER: an id, handle, link, `url` or `#N` of the last user list. Each
  command takes several.
- `unblock` is the same as `block --undo`.
- `remove-follower` asks `remove follower …? [y/N]` first when standard
  input is a terminal; scripts and MCP pass `-y` / `--yes`, or nothing is
  done. They may follow you again later.
- One user gives an Action; several give one Action each, in order. A
  user that fails gets `ok: false` with `error` (the error code) and
  `message`, and the rest go on; a login, a human check or a rate limit
  ends the run and marks the rest as not tried. The exit code is 1 when
  any failed. Platforms with a batch call send several users at once.
- `blocked` gives Users (`#N` then refers to them); `extra.blocked_at`
  says since when, where the platform tells it. `--since` / `--until` do
  not apply.
- Platforms restrict accounts that block or remove followers in bulk:
  space the work out with `--interval`.

More: `atp guide interact`
