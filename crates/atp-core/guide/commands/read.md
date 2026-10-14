Shows one item in full: a post, video, note, answer, article, product,
place ...; on chat platforms one message; on a drive one file or folder.

    atp bili read BV1xx411c7mD
    atp xhs read '#2'
    atp x read https://x.com/NASA/status/2104695667180380260

- POST takes what the platform shows: an id, a link or short link, the
  `url` of an earlier result, or `#N` for the N-th item of the last post
  list. A chat message is `CHAT/MESSAGE` (its `id` in `messages`) or a
  link.
- The result is a Post: `id`, `kind`, `title`, `text`, `url`, `author`,
  `created_at`, `metrics` (`views`, `likes`, `comments`, `shares`,
  `favorites` and platform counters), `media`, `tags`, `quoted` (the
  reposted or parent item), `price` where one applies, and platform
  fields in `extra`.
- The table view is a card; `--json` / `--yaml` show every field, and
  `--raw` adds the platform's untouched payload.
- `read` leaves the `#N` list alone, so `#N` keeps pointing at the list you
  came from.

### Cloud drives

`read` takes a path, an id or `#N` of a listing (some drives also a share
link) and prints a File: `id`, `name`, `path`, `dir`, `size`,
`updated_at`, `md5`, `share`.

More: `atp guide refs`, `atp guide output`
