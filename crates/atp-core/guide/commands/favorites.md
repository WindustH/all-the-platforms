`collections` lists folders, lists and similar containers (favorites
folders, bookmark lists, playlists ...). `favorites` lists saved or
bookmarked posts, all of them or one folder's.

    atp bili collections
    atp bili favorites --folder '#1' -n 100
    atp x favorites -n 200 -f jsonl > bookmarks.jsonl

- Without USER both show your own, which needs a login; with USER someone
  else's, where the platform makes them public. USER: an id, handle, link,
  `url` or `#N` of the last user list.
- `collections` gives Collections: `id`, `kind` (`folder`, `list` ...),
  `name`, `description`, `url`, `items` (how many it holds), `followers`,
  `owner`. They fill the `#N` list of collections, which `--folder` reads.
  `--since` / `--until` do not apply.
- `favorites --folder F` narrows the list to one folder: its id, or `#N`
  after `collections`.
- `favorites` gives Posts, and `#N` then refers to them. `--since` /
  `--until` judge when a post was published (`created_at`), not when it
  was saved, and a page of posts all published before `--since` ends the
  listing; on a list kept in saving order that can stop early.

Save a post with `favorite POST [--folder F]`; create, change and delete
folders and move posts between them with `folder-create`, `folder-edit`,
`folder-delete`, `folder-add`, `folder-remove` and `folder-move`.

More: `atp guide refs`
