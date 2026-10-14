## Logging in

### QR code

`atp douyin login` shows a QR code of Douyin's web passport
(login.douyin.com). Scan it with the Douyin app and confirm. When the
passport refuses this client (error 4031, "网站存在安全风险", or a page
instead of data), the login stops with `verification_required`; passing a
check in the browser does not help there, so log in with `--browser` or
`--cookie` instead (reads only, below).

### Phone and texted code

`atp douyin login-sms [PHONE]` asks Douyin's passport to text a code to
the number (mainland numbers as they are, others as `+CC NUMBER`; asked
when left out), then asks for the code on the terminal. When the passport
wants a captcha first (a slider or an identity check), the login stops
with `verification_required` and its page; ATP opens it in your browser,
and after you pass it the login runs again (with `--no-verify` the page is
only printed). It needs a terminal; ATP never reads texts itself.

### Write key

Before either login ATP creates a key pair and hands the public key to the
passport, as the browser does (Douyin's "ticket guard"); the passport
answers the login with a ticket for that key, which ATP keeps with the
session. Likes, favorites, comments and follows are signed with it, so
they work only on sessions made by these two logins. Whether the passport
issues that ticket to ATP's logins has not been confirmed; without it,
writes fail with `not_authenticated` ("this session cannot sign writes").

### Browser or cookie

Log in on douyin.com in a browser, then:

- `atp douyin login --browser` reads that browser's cookies.
- `atp douyin login --cookie '...'`: copy the `Cookie` header of a request
  to www.douyin.com. It must hold `sessionid`; the `UIFID` cookie a browser
  receives is needed for most reads (below).
- `ATP_DOUYIN_COOKIE='...'` passes such a header for one run; it is never
  saved.

Such sessions read everything a login allows but cannot write: the browser
keeps its write key to itself.

There are no other environment variables and no tokens.

### Without an account

Without a login, the hot boards (`hot`), user profiles (`user`) and live
rooms (`live`, `listen`) answer, and `read` and `download` fall back to the
share page's API (iesdouyin.com), which answers visitors. Everything else
keyed by a post or a list of posts (`user-posts`, `likes`, `mix`, `music`,
the recommended `feed` ...) needs the `UIFID` cookie a browser receives (a
logged-out browser's cookies work too, passed as `ATP_DOUYIN_COOKIE`);
without it Douyin answers "Uifid Not Found" and ATP reports
`not_authenticated`. Search, the following and friends feeds, followers,
following, `@douyin_id` lookups and the account's own lists
(`collections`, `favorites`, `history`, `notifications`, your `likes`)
need a login, and writes need ATP's own login (above).

### When the session ends

How long a session lasts is not known. When it ends, commands fail with
`not_authenticated` (Douyin's status 8, "not logged in") and the hint to
log in again.

### Risk control

- Every web API call is signed (`a_bogus`, plus `x-secsdk-web-signature`
  on the paths the web SDK protects), and carries the visitor ids `ttwid`
  and `s_v_web_id`, which ATP registers once and reuses for a week.
  Douyin still scores clients: an empty answer or a page instead of data
  is `verification_required` (Douyin refuses the call to visitors or
  flagged clients). Slow down, open douyin.com in the browser, and retry
  later; log in with `--browser`, or use a residential network in mainland
  China. On a terminal ATP opens douyin.com in a browser for the check (see
  `atp guide login`). ATP keeps at least 800 ms between requests.
- Writes also carry device traits (`x-tt-session-dtrait`, from a captured
  Chrome profile) and the secsdk CSRF token, and ATP waits a random 2 to
  6 s before each. They are untested against the live site: another client
  that reproduces them reports likes sent without device traits answered
  with HTTP 403 and the session logged out. Try them on an account you can
  afford to log in again.

## References

- Posts: the aweme id, `https://www.douyin.com/video/<id>`, `/note/<id>`
  (image posts), links with `?modal_id=<id>`, `iesdouyin.com/share/...`
  pages, `https://v.douyin.com/...` short links or shared text containing
  one.
- Users: the `sec_uid` (`MS4wLjABAAAA...`), `https://www.douyin.com/user/<sec_uid>`,
  or `@douyin_id` (looked up by search, so it needs a login).
- Mixes (合集): the id or a `douyin.com/collection/<id>` link; music: the id
  or a `douyin.com/music/<id>` link; live rooms: the `web_rid`, `live:<web_rid>`
  or a `live.douyin.com/<id>` link.

## Commands

### search

`--sort general|likes|latest` plus at most one `--filter`: a time
(`day`, `week`, `half-year`), a type (`video`, `images`), a length
(`short`, `medium`, `long`) or a range (`unseen`, `following`). Without a
login Douyin usually returns nothing, reported as `verification_required`.
`search -t user QUERY` finds users.

### hot

The hot boards (`--category all|entertainment|society|challenge`), each
word with its heat (`metrics.heat`) and views. When the signed web board is
walled, the open billboard (words and heat only) stands in for `all`.

### feed

`--type recommend` (default), `following` or `friends`; the last two need a
login.

### likes

`likes [USER]`: posts a user liked (public likes only); yours when USER is
omitted.

### collections, favorites

Your own account only. `collections` lists your folders of saved posts;
`favorites [--folder ID]` your saved posts, all or those of one folder.

### history

What you watched on this account.

### notifications

The notice center (互动消息) of your account:
`--type all|follower|mention|comment|like|danmaku`.

### like, unlike, favorite, unfavorite

Like a post or save it to your favorites (收藏), or undo either. Saving
into a folder (`--folder`) is not offered. Needs ATP's own login (see
Logging in).

### comment, delete-comment

`comment POST TEXT` comments on a post; `--reply-to COMMENT` answers one of
its comments (the id `comments` lists). Comments are text only.
`delete-comment POST COMMENT` asks first (`-y` in scripts). Both need
ATP's own login; comments are signed with its key.

### follow, unfollow

Follow or unfollow a user (a `sec_uid`, a profile link or `@douyin_id`).
Needs ATP's own login.

### live

`live ROOM`: title, anchor, audience and the FLV / HLS stream addresses in
`extra.streams`.

### listen

`listen ROOM …` follows live rooms (a `web_rid`, `live:<web_rid>` or a
live.douyin.com link) over Douyin's webcast websocket, without a login.
Each event is a post whose `kind` is `message` (danmaku), `gift` (name ×
count, `extra.count`), `like` (`extra.likes`, `extra.likes_total`),
`enter` (someone joined, `extra.audience`), `follow`, `share`, `stats`
(the audience count, `extra.viewers`) or `offline` (the live ended, which
ends the stream); `extra.chat` is `live:<web_rid>`. A room that is not
live is `not_found`.

### login-sms

Logs in with a phone number and a texted code; see `atp guide douyin
login`.

### mix, music, related

`mix MIX` and `music MUSIC` list the posts of a mix or a music; `related
POST` the posts Douyin recommends next to one.

### download

Videos at the highest bitrate without watermark, every image of an image
post (with live-photo clips) and its music; `--audio-only` saves the music.

## Limits

- No publishing (`post`) or deleting posts: the creator platform's upload
  flow (ImageX / VOD uploads, then `create_v2`) can be reproduced over
  plain HTTP, but the reference that does so reports `create_v2` answering
  an empty body even with a complete write key and device traits.
- Writes are untested against the live site (see Risk control).
- No creator analytics yet.
