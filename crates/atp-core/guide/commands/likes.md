Lists the posts a user liked: yours without USER, someone else's where the
platform makes their likes public.

    atp x likes -n 100
    atp douyin likes '#1'             # after a user list

- USER: an id, handle, link, `url` or `#N` of the last user list. Your own
  likes need a login.
- The result is Posts, and `#N` then refers to them: `atp x read '#3'`,
  `atp x unlike '#3'`.
- `-n` and `--cursor` page as everywhere; repeated items are dropped.
- `--since` / `--until` judge when a post was published (`created_at`),
  not when it was liked. A page of posts all published before `--since`
  ends the listing, so on a list in liking order the window can stop
  early.

Like or unlike with `like POST [--undo]`.

More: `atp guide refs`
