## References

- Posts: `https://www.toutiao.com/article/<id>/`, `/video/<id>/`, `/w/<id>/`
  (微头条), `/group/<id>/`, the same on `m.toutiao.com`, `m.toutiao.com/is/…`
  short links, or a bare group id.
- Users: `https://www.toutiao.com/c/user/token/MS4w…/` or the `MS4w…`
  token (`url` of an author in any list). Numeric ids (authors of a `read`)
  only work for `follow`.

## Reading

- `hot`: the hot board (topics with their heat).
- `feed`: the recommend feed (default, paged), or one page of a channel with
  `--type hot|tech|finance|world|sports|entertainment|military|video|all`.
- `read` shows the article text, its images and, for videos, the best MP4;
  `download` saves them (image and video links expire within a day).
- `comments POST` and `replies POST COMMENT` need no login. When
  www.toutiao.com answers comments with its script challenge, atp reads
  them from the host Toutiao's share pages use.
- `user USER` and `user-posts USER` come from the signed user feed; when it
  is refused, `user-posts` falls back to the user's 微头条.
- `search` reads the result page (`--filter information|video|weitoutiao|synthesis`).
  Toutiao often answers it with a slide captcha: pass it once in a browser
  and `login --browser`, or retry later.

## Logging in

`atp toutiao login` shows a QR code for the Toutiao app. Risk control may
ask for an identity check with the Douyin app first. `--browser` and
`--cookie` (`sessionid`) work too.

## With a login

- `insights`: daily impressions, views, follower changes, comments and likes
  from the 头条号 creator backend; `insights POST` for one of your posts.
- `like POST [--undo]`, `follow USER_ID` and `following USER` use endpoints
  from older references: they may need updating.

## Limits

No direct messages (Toutiao has no web API for them), no publishing.
