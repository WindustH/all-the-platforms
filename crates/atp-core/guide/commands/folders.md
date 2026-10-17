Manage your favorites folders: the containers `collections` lists and
`favorite --folder` saves into (收藏夹, boards, bookmark folders). All
need a login.

    atp bili folder-create 'Watch again' --desc 'for later' --private
    atp bili collections
    atp bili folder-edit '#2' --name 'Music videos' --public
    atp bili folder-add '#2' BV1xx411c7mD '#3'
    atp bili favorites --folder '#2' -n 50
    atp bili folder-move '#2' '#4' '#1' '#7'
    atp bili folder-move 1052622027 1052622127 '#3' --copy
    atp bili folder-remove '#2' '#5'
    atp bili folder-delete '#4' '#5' -y

- FOLDER, FROM and TO: a folder id, or `#N` of the last collection list
  (`collections`). POST: an id, link, `url` or `#N` of the last post list
  (`favorites --folder F` lists a folder's posts).
- `folder-create NAME` and `folder-edit FOLDER` take `--desc TEXT`,
  `--private` (and `--public` on `folder-edit`) and `--cover IMAGE` where
  the platform has them (`folder-description`, `folder-privacy`,
  `folder-cover` in `atp platforms`); elsewhere they are hidden and
  refused. What `folder-edit` is not given stays as it was.
- `folder-delete` asks `delete folder …? [y/N]` first when standard input
  is a terminal; scripts and MCP pass `-y`. What happens to the posts in
  a deleted folder differs per platform (see its notes).
- `folder-add`, `folder-remove`, `folder-move` and `folder-delete` take
  several posts (or folders) and give an Action each, in order; one gives
  a single Action. A failed one has `ok: false`, `error` and `message`;
  the exit code is then 1.
- `folder-move FROM TO POST…` uses the platform's own move where it has
  one; elsewhere it adds the posts to TO, then takes the ones added out
  of FROM. `--copy` keeps them in FROM. A post that reached TO but stayed
  in FROM is reported as failed, saying so.
- `folder-create` gives an Action with the new folder's `id` (and `url`
  where it has one).

More: `atp guide interact`, `atp guide refs`
