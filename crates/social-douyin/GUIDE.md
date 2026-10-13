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

## Logging in

`social douyin login --browser` (a browser logged in on douyin.com) or
`--cookie '...'`. There is no QR login: Douyin's passport flow needs a
browser.

Without a login only the hot boards (`hot`) and user profiles (`user`)
answer. Everything keyed by a post or a list of posts (`read`,
`user-posts`, `download` ...) needs the `UIFID` cookie a browser receives
(a logged-out browser's cookies work too, passed as `SOCIAL_DOUYIN_COOKIE`);
search, feeds, followers and the account's own lists need a login.

Every web API call is signed (`a_bogus`, plus `x-secsdk-web-signature` on the
paths the web SDK protects). Douyin still scores clients: an empty answer or
a captcha means slow down, open douyin.com in the browser, and retry later.

## Reading

- `search QUERY [--sort general|likes|latest] [--filter day|week|half-year|video|images|short|medium|long|unseen|following]`,
  `search -t user QUERY`.
- `hot [--category all|entertainment|society|challenge]`: the hot boards,
  each word with its heat (`metrics.heat`) and views.
- `feed [--type recommend|following|friends]`.
- `read POST`, `comments POST`, `replies POST COMMENT`.
- `user USER`, `user-posts USER`, `likes [USER]` (public likes only),
  `followers USER`, `following USER`.
- `collections` lists your folders of saved posts; `favorites [--folder ID]`
  your saved posts; `history` what you watched; `notifications
  [--type all|follower|mention|comment|like|danmaku]`.
- `mix MIX`, `music MUSIC`, `related POST`, `live ROOM` (title, anchor,
  audience and the FLV / HLS stream addresses in `extra.streams`).

## Downloads

Videos at the highest bitrate without watermark, every image of an image
post (with live-photo clips) and its music; `--audio-only` saves the music.

## Limits

No likes, comments, follows, favorites or publishing: Douyin checks writes
with ticket-guard headers signed by keys the browser registers at login,
which a copied cookie does not carry. No creator analytics yet.
