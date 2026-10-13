## References

- Posts: `https://weibo.com/<uid>/<id>`, `https://weibo.com/detail/<mid>`,
  `https://m.weibo.cn/status/<id>`, `https://m.weibo.cn/detail/<mid>`, the
  numeric id (`mid`, 16 digits) or its short form (`bid`, e.g. `RmbnfkgUR`).
- Users: the numeric id, `@screen name` (or the bare name), a personal
  domain, or a `weibo.com/u/<uid>`, `weibo.com/n/<name>` or `m.weibo.cn/u/<uid>` link.
- Comments: the numeric comment id (`comments` lists them).

## Logging in

`atp weibo login` shows a QR code for the Weibo app (我的 → 扫一扫).
`--browser` reuses a weibo.com browser session and `--cookie` takes the
cookie header of weibo.com (it needs `SUB`).

Without a login, a visitor cookie from Sina's visitor system is used: hot
lists, search, posts, comments, likers, profiles, a user's posts, followers
and following all work. Reposts lists and everything about your own account
need a login.

## Reading

- `hot --category realtime` (热搜, the default), `posts` (热门微博), and the
  boards `entertainment`, `life`, `social`, `tech`, `sports`, `acg`,
  `trend`, `explode` (these two come back empty without a login) and `mine`
  (yours, logged in). Hot-search entries are posts of kind `trend` with the
  heat in `metrics.views`.
- `feed --type following|latest|original|video|hot` (logged out: `hot`).
- `search --sort general|latest|hot`, `--filter video|image`;
  `-t user`, `-t topic` (with discussion and read counts).
- `comments --sort hot|time`, `--all --replies` for whole threads.
- `topic NAME` lists posts under a hashtag; `groups` and `group GID` read
  your feed groups; `folders` lists favorite folders for `favorites --folder`.

## Interactions

- `post TEXT [--title T] [-i IMAGE ...] [--topic T]` (up to 18 pictures;
  topics become `#T#`); `post TEXT --quote POST` and `repost POST [TEXT]
  [--comment]` repost.
- `comment POST TEXT [--reply-to COMMENT]`, `delete-comment POST COMMENT`,
  `like`, `favorite`, `follow`, `delete` (your post).
- Weibo asks for a slider check when an account writes too much; the
  command then fails with `verification_required`.

## Analytics

`insights` reports what weibo.com shows: lifetime followers, posts and the
likes, comments and reposts received, plus yesterday's reads and
interactions. `insights POST` gives a post's counters (reads where Weibo
shows them, video plays). The creator data center (me.weibo.com) has no
usable web API, so there are no daily trends or audience breakdowns.

## Downloads

Pictures at their largest size (live photos with their motion clip, GIFs
as video) and videos in the best quality offered.
