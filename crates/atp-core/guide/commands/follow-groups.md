Follow groups (关注分组) are lists you sort the people you follow into.
`follow-groups` lists yours, `follow-group GROUP` the people in one; the
other commands create, rename and delete groups and put people in or take
them out. All need a login.

    atp bili follow-groups
    atp bili follow-group '#2' -n 100
    atp bili follow-group-create 'Game streams'
    atp bili follow-group-rename '#3' 'Streams'
    atp bili follow-group-add '#3' 946974 '#5'
    atp bili follow-group-remove 194111 946974
    atp bili follow-group-delete '#3' -y

- `follow-groups` gives Collections of kind `group`: `id`, `name`,
  `items` (how many people it holds). They fill the `#N` list of
  collections, which GROUP reads; GROUP is otherwise the group's id (some
  platforms also take its exact name).
- `follow-group` gives Users and takes `--sort` where the platform orders
  groups (`recent`, `visited` ...); `#N` then refers to the users.
  `--since` / `--until` do not apply.
- `follow-group-add` keeps each user's other groups; `follow-group-remove`
  takes them out of this group only, and they stay followed. Both take
  several users (`#N` of the last user list) and give an Action each, as
  `follow` does with several users.
- `follow-group-delete` asks first on a terminal (`-y` in scripts and
  MCP); the people in it stay followed.
- On some platforms the special-follow list (特别关注) or the default
  group are groups too, with ids of their own; the platform's notes say
  which.

More: `atp guide interact`
