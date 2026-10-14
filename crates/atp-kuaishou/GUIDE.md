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

## Logging in

`atp kuaishou login` shows a QR code to scan with the Kuaishou app;
`atp kuaishou login --browser` (a browser logged in on kuaishou.com) or
`--cookie '...'` import a web session. The live site and the creator center
take their own tickets; they are minted from the session's `passToken` the
first time a command needs them.

Without a login the hot board, the recommended feed, `read`, `comments`
and the live rooms answer; search, profiles and users' posts want a login.
Kuaishou puts detail pages, comments and some lists behind a slider captcha
for clients it does not trust: the error (`verification_required`) carries
the captcha link. Pass it in a browser, then import that browser's cookies
with `login --browser`. `read` falls back to the mobile share page when the
detail page is behind the slider.

## Reading

- `hot`: the hot board, each word with its heat (`metrics.heat`), rank and
  the photos about it (`extra.photo_ids`).
- `feed`: the recommended feed of the home page.
- `search QUERY`, `search -t user QUERY`.
- `read POST`, `comments POST`, `replies POST COMMENT`; `related POST`.
- `user USER`, `user-posts USER`.
- `followers`, `following`, `likes`, `favorites`: your own only (the web
  shows these lists to their owner).
- `rooms` lists live rooms; `live ROOM` shows one with its stream
  addresses (`extra.streams`); `gifts ROOM` its gift panel; `replays USER`
  a user's past streams.
- `listen live:ROOM` prints a live room's comments, gifts and notices
  (needs a login).

## Creator center

`insights` gives the overview of the creator center's home;
`insights POST` the counters of one of your posts; `works` lists your works
with their review state (`extra.status`); `unread` the unread notification
counts.

## Writing

`like POST [--undo]`, `comment POST TEXT [--reply-to COMMENT]`,
`follow USER [--undo]`. They use the web's own calls but are not verified
against a live account yet; no publishing, deleting or favorites.

## Downloads

The highest-resolution video, or every image of an image set.
`--audio-only` is not offered.
