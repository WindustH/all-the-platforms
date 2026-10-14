## Logging in

There is no QR login: Douyin's passport flow needs a browser. Log in on
douyin.com in a browser, then:

- `atp douyin login --browser` reads that browser's cookies.
- `atp douyin login --cookie '...'`: copy the `Cookie` header of a request
  to www.douyin.com. It must hold `sessionid`; the `UIFID` cookie a browser
  receives is needed for most reads (below).
- `ATP_DOUYIN_COOKIE='...'` passes such a header for one run; it is never
  saved.

There are no other environment variables and no tokens.

### Without an account

Without a login, the hot boards (`hot`) and user profiles (`user`)
answer, and `read` and `download` fall back to the share page's API
(iesdouyin.com), which answers visitors. Everything else keyed by a post or
a list of posts (`user-posts`, `likes`, `mix`, `music`, the recommended
`feed` ...) needs the `UIFID` cookie a browser receives (a logged-out
browser's cookies work too, passed as `ATP_DOUYIN_COOKIE`); without it
Douyin answers "Uifid Not Found" and ATP reports `not_authenticated`.
Search, the following and friends feeds, followers, following,
`@douyin_id` lookups and the account's own lists (`collections`,
`favorites`, `history`, `notifications`, your `likes`) need a login.

### When the session ends

How long a session lasts is not known. When it ends, commands fail with
`not_authenticated` (Douyin's status 8, "not logged in") and the hint to
log in again with `--browser` or `--cookie`.

### Risk control

Every web API call is signed (`a_bogus`, plus `x-secsdk-web-signature` on
the paths the web SDK protects), and carries the visitor ids `ttwid` and
`s_v_web_id`, which ATP registers once and reuses for a week. Douyin still
scores clients: an empty answer or a page instead of data is
`verification_required` (Douyin refuses the call to visitors or flagged
clients). Slow down, open douyin.com in the browser, and retry later; log
in with `--browser`, or use a residential network in mainland China. On a
terminal ATP opens douyin.com in a browser for the check (see
`atp guide login`). ATP keeps at least 800 ms between requests.

## References

- Posts: the aweme id, `https://www.douyin.com/video/<id>`, `/note/<id>`
  (image posts), links with `?modal_id=<id>`, `iesdouyin.com/share/...`
  pages, `https://v.douyin.com/...` short links or shared text containing
  one.
- Users: the `sec_uid` (`MS4wLjABAAAA...`), `https://www.douyin.com/user/<sec_uid>`,
  or `@douyin_id` (looked up by search, so it needs a login).
- Mixes (合集): the id or a `douyin.com/collection/<id>` link; music: the id
  or a `douyin.com/music/<id>` link; live rooms: the `web_rid` or a
  `live.douyin.com/<id>` link.

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

### mix, music, related

`mix MIX` and `music MUSIC` list the posts of a mix or a music; `related
POST` the posts Douyin recommends next to one.

### live

`live ROOM`: title, anchor, audience and the FLV / HLS stream addresses in
`extra.streams`.

### download

Videos at the highest bitrate without watermark, every image of an image
post (with live-photo clips) and its music; `--audio-only` saves the music.

## Limits

- No likes, comments, follows, favorites or publishing: Douyin checks
  writes with ticket-guard headers signed by keys the browser registers at
  login, which a copied cookie does not carry.
- No creator analytics yet.
