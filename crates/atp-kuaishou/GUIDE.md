## Logging in

### QR code

`atp kuaishou login` shows a QR code (id.kuaishou.com). Scan it with the
Kuaishou app, then confirm the login on the phone; cancelling on the phone
ends the login with `not_authenticated`. The session keeps `userId`,
`passToken` and the www site's tickets.

### Browser or cookie

- `atp kuaishou login --browser` imports the session of a browser logged
  in at kuaishou.com.
- `atp kuaishou login --cookie '...'`: copy the `Cookie` header of a request
  to www.kuaishou.com. It must hold `userId`; keep `passToken` in it too
  for the live site and the creator center (below).
- `ATP_KUAISHOU_COOKIE='...'` passes such a header for one run; it is never
  saved.

There are no other environment variables and no tokens.

The live site and the creator center take their own tickets; they are
minted from the session's `passToken` the first time a command needs them
(`listen`, `insights`, `works`, `unread`). A session with neither those
tickets nor `passToken` gets `not_authenticated` there.

### Without an account

Without a login the hot board, the recommended feed, `read`, `comments`
and the live rooms answer; search, profiles and users' posts want a login.
`followers`, `following`, `likes` and `favorites` show only your own
lists; they, `listen`, the creator center and every write need a login.
ATP keeps a visitor identity (`did` and the web fingerprint cookies) for a
week, so runs without a login reuse it.

### When the session ends

How long a session lasts is not known. When it ends, commands fail with
`not_authenticated` ("Kuaishou wants a login for this (result 109)" or
"Kuaishou does not know this session") and the hint to log in with the QR
code or `--browser`.

### Risk control

- Kuaishou puts detail pages, comments and some lists behind a slider
  captcha for clients it does not trust: the error
  (`verification_required`) carries the captcha link. On a terminal ATP
  opens that link in a browser for you to pass (see `atp guide login`);
  otherwise pass it in a browser yourself, then import that browser's
  cookies with `login --browser`, or wait a while. `read` falls back to
  the mobile share page when the detail page is behind the slider.
- "Refused as risky" (result 2) is `rate_limited`: Kuaishou flags fast or
  cookie-less clients. Wait a while, or log in with `--browser`.
- ATP keeps at least 1.5 s between requests and waits a random 0.8 to 2 s
  before every write.

## References

- Posts: the photo id (`3x…`), `https://www.kuaishou.com/short-video/<id>`,
  `https://v.m.chenzhongtech.com/fw/photo/<id>` share pages,
  `live.kuaishou.com/u/<user>/<id>` links, `https://v.kuaishou.com/...` share
  links or shared text containing one.
- Users: the eid (`3x…`), `https://www.kuaishou.com/profile/<eid>`, or `me`.
  Live ids (`live.kuaishou.com/u/<id>`) are a separate set: a live link is
  not a profile.
- Live rooms: the anchor's live id or a `live.kuaishou.com/u/<id>` link
  (`live:<id>` for `listen`).

## Commands

### hot

The hot board, each word with its heat (`metrics.heat`), rank and the
photos about it (`extra.photo_ids`).

### feed

The recommended feed of the home page.

### search

`search QUERY`, `search -t user QUERY`; both want a login.

### read, related

`read` falls back to the mobile share page when the detail page is behind
the slider. `related POST` lists the posts Kuaishou recommends next to one
(one batch).

### followers, following, likes, favorites

Your own only (the web shows these lists to their owner); another user is
`unsupported`. `favorites` takes no `--folder`.

### rooms, live, gifts, replays

`rooms` lists the live rooms of the live home page, live ones first; `live
ROOM` shows one with its stream addresses (`extra.streams`); `gifts ROOM`
its gift panel (prices in Kuaishou coins); `replays USER` a user's past
streams.

### listen

`listen live:ROOM` prints a live room's comments, gifts and notices (not
likes or shares). It needs a login: the live site hands the socket token
only to sessions with its tickets.

### insights

The creator center: `insights` gives the overview of the creator center's
home; `insights POST` the counters of one of your posts (views, likes,
comments, shares, favorites), looked up among your 100 latest works of the
past year. `--days` is not used.

### works

Lists your works of the past year with their review state
(`extra.status`).

### unread

The unread notification counts of the creator center.

### like, unlike, comment, follow, unfollow

`like POST [--undo]`, `comment POST TEXT [--reply-to COMMENT]`,
`follow USER [--undo]`. They use the web's own calls but are not verified
against a live account yet; `like` and `comment` read the post first.
`comment` takes no `--image`.

### download

The highest-resolution video, or every image of an image set.
`--audio-only` is not offered.

## Limits

- No publishing, deleting or favoriting.
- ATP speaks the Kuaishou web: the www, live and creator-center calls,
  each signed the way its web page signs it. The Kuaishou app's own API,
  where private messages live, is signed by the app itself with a
  different algorithm, so ATP has no direct messages and does not use app
  endpoints.
