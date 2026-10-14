## Logging in

### QR code

`atp toutiao login` shows a QR code of Toutiao's SSO passport
(sso.toutiao.com). Scan it with the Toutiao app and confirm. Risk control
may ask for an identity check with the Douyin app first: the login stops
with `verification_required`; finish the check in the app, then log in
again. When the passport answers with a page instead of data (risk
control), use `--browser` instead.

### Browser or cookie

- `atp toutiao login --browser` reads the cookies of a browser logged in at
  toutiao.com.
- `atp toutiao login --cookie '...'`: copy the `Cookie` header of a request
  to www.toutiao.com; it must hold `sessionid`.
- `ATP_TOUTIAO_COOKIE='...'` passes such a header for one run; it is never
  saved.

There are no other environment variables and no tokens.

### Without an account

Reading needs no login: `hot`, `feed`, `search`, `read`, `comments`,
`replies`, `user`, `user-posts` and `download`. A login is needed for
`insights` (the 头条号 creator backend), `like` and `follow`.

### When the session ends

How long a session lasts is not known. When it ends, commands fail with
`not_authenticated` ("Toutiao did not recognize the session", or the
creator backend's own message) and the hint to log in with the QR code or
`--browser`.

### Risk control

- The recommend feed and user feeds are signed with ByteDance's `a_bogus`
  and carry the visitor ids `ttwid` and `s_v_web_id` (kept for a week) and
  an `msToken`. A refused signature answers an empty body, reported as
  `verification_required` ("Toutiao refused a signed request"): retry
  later, or log in.
- Search often answers with a slide captcha (below).
- ATP keeps at least 1 s between requests and waits a random 0.8 to 2 s
  before `like` and `follow`.

## References

- Posts: `https://www.toutiao.com/article/<id>/`, `/video/<id>/`, `/w/<id>/`
  (微头条), `/group/<id>/`, the same on `m.toutiao.com`, `m.toutiao.com/is/…`
  short links, or a bare group id.
- Users: `https://www.toutiao.com/c/user/token/MS4w…/` or the `MS4w…`
  token (`url` of an author in any list). Numeric ids (authors of a `read`)
  only work for `follow`.

## Commands

Reads go through the mobile pages (m.toutiao.com): the www pages hide their
data behind a script challenge.

### hot

The hot board (topics with their heat).

### feed

The recommend feed (default, paged), or one page of a channel with
`--type hot|tech|finance|world|sports|entertainment|military|video|all`.

### read, download

`read` shows the article text, its images and, for videos, the best MP4;
`download` saves them (image and video links expire within a day).

### comments, replies

Need no login. When www.toutiao.com answers comments with its script
challenge, ATP reads them from the host Toutiao's share pages use.

### user, user-posts

Come from the signed user feed; when it is refused, `user-posts` falls back
to the user's 微头条 (one page).

### search

Reads the result page (`--filter information|video|weitoutiao|synthesis`).
Toutiao often answers it with a slide captcha (`verification_required`):
pass it once at so.toutiao.com in a browser and `login --browser`, or retry
later. On a terminal ATP opens toutiao.com in a browser that carries the
session (see `atp guide login`).

### insights

Needs a login: daily impressions, views, follower changes, comments and
likes from the 头条号 creator backend over `--days`; `insights POST` for
one of your posts (its lifetime numbers).

### like, unlike, follow, following

`like POST [--undo]`, `follow USER_ID` and `following USER` use endpoints
from older references: they may need updating. `follow` takes the numeric
user id (the `id` of a post's author) and cannot be undone here.

## Limits

No direct messages (Toutiao has no web API for them), no publishing.
