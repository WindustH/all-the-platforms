Juejin (掘金) through the JSON API of api.juejin.cn that the site itself
calls: articles (their whole Markdown rendered as text), pins (沸点), the
hot ranking (热榜) and the home feeds, search, comments with their
replies, users with their articles, pins, followers and followings, and
collection sets. Reading needs no account. A login adds 点赞, collecting
articles, comments and replies, following, and the daily check-in (签到).

## Logging in

### Browser or cookie

- `atp juejin login --browser` reads the cookies of a browser logged in at
  juejin.cn.
- `atp juejin login --cookie '...'`: the `Cookie` header of a request to
  juejin.cn or api.juejin.cn; it must hold `sessionid`.
- `ATP_JUEJIN_COOKIE='...'` passes such a header for one run; it is never
  saved.

There is no QR login: the QR code of Juejin's login dialog is requested
with a signature of ByteDance's passport SDK that ATP does not reproduce.
There are no tokens and no other environment variables.

### Without an account

`search`, `hot`, `feed`, `read`, `comments`, `replies`, `user`,
`user-posts`, `followers`, `following`, `collections USER`, `download` and
`pins` work without logging in.

### With an account

`whoami`, `collections` without a user, `like` / `unlike`, `favorite` /
`unfavorite`, `comment` (and replies with `--reply-to`), `follow` /
`unfollow`, and `check-in`.

### How long it lasts

How long a Juejin session (`sessionid`) lasts is not published. When it
ends, commands that need it fail with
`not_authenticated` (Juejin's `403 must login`) and the hint to log in
again.

### Risk control

- Every request carries Juejin's app id (`aid=2608`), a visitor id
  (`uuid`, kept for a month) and `spider=0`, as the site's own requests do.
- `check-in`, `like`, `comment` and replies are among the requests the
  site signs with ByteDance's `a_bogus`; ATP signs them the same way (with
  an `msToken`). Juejin loads an older version of that signing SDK than
  Douyin, without a page id; whether Juejin accepts ATP's signature has not
  been verified with a logged-in account. A refused signature shows as
  Juejin's own error.
- When api.juejin.cn answers with its bot check instead of data, commands
  fail with `verification_required` and juejin.cn as the page to open; on
  a terminal ATP opens it in your Chrome, you pass it by hand, and the
  command runs again.
- ATP keeps at least 0.6 s between requests and waits 0.8 to 2 s before
  each change to your account.

## References

- Articles: `https://juejin.cn/post/<id>` (also juejin.im), `article:<id>`
  or the bare id.
- Pins: `https://juejin.cn/pin/<id>` or `pin:<id>`.
- Users: `https://juejin.cn/user/<id>` (any of its tabs) or the id.
- Comments: the `id` that `comments` lists, for `replies` and `--reply-to`.
- Collection sets: the `id` that `collections` lists, for `--folder`.
- `#N` works after any list.

## Commands

### search

Articles; `--type user` searches users. `--sort relevance` (the default),
`newest` or `hottest`.

### hot

The hot ranking (热榜, fifty articles) of `--category all` (the default),
`backend`, `frontend`, `android`, `ios`, `ai`, `tools`, `career` or
`reading`, each with its place (`extra.rank`) and heat (`extra.heat`).

### feed

`--type recommend` (the home page's recommended articles, the default),
`newest`, `pins` (recommended pins) or `pins-hot`.

### read

An article with its whole text: the Markdown rendered as plain text
(headings, lists, quotes and tables without markers, code blocks
indented, links as `text (url)`, images as `[image: …]` with their links in
`media`), its category and tags, views, likes, comments, collections and
reading time. A pin shows its text, pictures, topic and link.

### comments, replies

`comments POST` lists comments with their first replies; `replies POST
COMMENT` all replies of one comment. Juejin has no comment order to
choose.

### user, user-posts, followers, following

A profile: name, title, company, level, followers, following, articles,
likes received, views. `user-posts USER`: the user's articles, newest
first.

### collections

A user's collection sets (收藏集), yours without USER.

### like, unlike

点赞 an article or pin, or take it back.

### favorite, unfavorite

`favorite ARTICLE [--folder SET]` collects an article into a collection
set: your first one (the default 收藏集) unless `--folder` names another;
`unfavorite` takes it out again.

### comment

`comment POST TEXT` comments on an article or pin; `--reply-to COMMENT`
replies to a comment. Images are not supported.

### follow

`follow USER [--undo]`.

### pins

`pins USER`: a user's pins, newest first.

### check-in

The daily check-in (签到): answers the points gained and the total, or
that today's check-in was already done.

## Limits

No QR login, no publishing articles or pins, no deleting, no
notifications, no booklets (小册), courses or the lottery, no comment
images, and no following feed (it needs a login ATP has not verified).
