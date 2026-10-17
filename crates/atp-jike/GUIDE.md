Jike (即刻) through the API its web app (web.okjike.com) calls: posts
and reposts with their pictures, videos and comments, the following feed
and the recommended feed (发现), search across posts, users and topics,
users with their posts, followers and following, topics (圈子), saved
posts (收藏) and notifications. With a login: like, save, comment and
reply (with a picture), repost, follow, post (text, up to nine pictures,
a topic) and delete your posts and comments.

## Logging in

### QR code

`atp jike login` shows the QR code of the web app's login page. Scan it
with the Jike app and confirm the login on the phone. ATP then trades the refresh token Jike hands out for a fresh
access token and refresh token, as the web app does, and keeps both in
its session file (readable by you only). How long the QR code stays
valid is not known; ATP waits up to three minutes.

### Tokens

`atp jike login --token 'REFRESH_TOKEN'` (or `'ACCESS_TOKEN|REFRESH_TOKEN'`;
`-` reads it from stdin) takes the tokens of a browser logged in at
web.okjike.com: the web app keeps them in the browser's local storage
under `JK_REFRESH_TOKEN` and `JK_ACCESS_TOKEN` (developer tools, Application,
Local Storage, `https://web.okjike.com`). They are not cookies, so
`--browser` and `--cookie` cannot import them. A refresh token alone is
traded for a fresh pair at once, which may end the browser's session,
since Jike rotates refresh tokens; give both to leave the browser logged
in until the access token runs out.

### Without an account

`read`, `comments`, `replies` and `download` work through the API without
a login; `user`, `user-posts` (the latest posts only, one page) and
`topic` come from the public pages of m.okjike.com. Jike answers
`search` and `hot` only to a logged-in account; so do `feed`,
`collections`, `favorites`, `notifications`, `unread` and every write.
Whether `followers` and `following` answer without a login is not
known; when Jike refuses a call, it fails with `not_authenticated`.

### How long it lasts

The access token is short-lived and renewed with the refresh token
whenever Jike answers that it expired (HTTP 401 or code `E101`), as the
web app does; the new pair is saved at once. How long a refresh token
lasts is not published. When it is refused, commands fail with
`not_authenticated` and the hint to log in again. Logging out of the
web app does not end ATP's session, and the other way round.

### Risk control

- ATP keeps at least 1.2 s between requests, and waits 2 to 6 s before
  each write.
- Jike answers 429 (操作过于频繁) when an account acts too often: that is
  `rate_limited`; wait before retrying.
- When Jike wants a person to read or do something first (code `E202`
  with an alert, for example after unusual activity), commands fail with
  `verification_required`, Jike's message and the page the alert links
  to (else web.okjike.com). Open it, do what it asks, and retry later.
- Posting, commenting or following in bursts or on a schedule looks
  automated and can get an account limited or banned; Jike does not
  publish its thresholds. Deletes ask for confirmation (`--yes` skips
  it).

### Environment

There are no environment variables; tokens come only from `login`.

## References

- Posts: `https://web.okjike.com/u/<username>/post/<id>`, reposts
  `…/repost/<id>` (what lists print), the old web links
  `https://web.okjike.com/originalPost/<id>`, share links
  `https://m.okjike.com/originalPosts/<id>` and `/reposts/<id>`, app links
  `jike://page.jk/originalPost/<id>`, `post:<id>`, `repost:<id>`, or the
  bare id (24 hex digits; tried as a post, then as a repost).
- Comments: their id, as `comments` lists it.
- Users: `https://web.okjike.com/u/<username>`,
  `https://m.okjike.com/users/<username>`, `@username` or the username
  (Jike's usernames often look like `3EE02BC9-C5B3-4209-…`).
- Topics (圈子): `https://web.okjike.com/topic/<id>`,
  `https://m.okjike.com/topics/<id>`, `topic:<id>` or the id.
- `#N` works after any list.

## Commands

### hot

The recommended feed of the web app's 发现 page (needs a login). It takes
no category.

### feed

The posts, reposts and updates (follows …) of the people you follow, as
the web app's 动态 page shows them.

### search

`search WORDS` (needs a login) returns posts; `--type user` the users and
`--type topic` the topics the same search finds. Jike answers all three in one list, so
each page may hold fewer items of one kind than asked for.

### comments

`--sort hot` (by likes, the default) or `--sort latest`. Each comment
carries the hot replies Jike picks (`replies`); `replies POST COMMENT`
lists all of one comment's replies.

### user-posts

With a login, everything a user posted and reposted, page after page.
Without one, only the latest posts their public page shows.

### collections

`collections [USER]`: the topics (圈子) a user joined, yours without a
user (needs a login). List a topic's posts with `topic`.

### topic

`topic TOPIC [--tab TAB]` lists a topic's posts from one of its tabs (the
first by default); `topic TOPIC --info` shows the topic with its member
count and tabs. Without a login: the topic and its latest posts from its
public page, without tabs.

### favorites, favorite, unfavorite

Your saved posts (收藏), one list without folders.

### block, unblock

`block USER…` / `unblock USER…` (拉黑) as the profile menu does, one call
per user. Jike's web app lists no blocked users, so there is no
`blocked`.

### comment

`comment POST TEXT [--reply-to COMMENT] [-i PICTURE]`: one PNG or JPEG
picture at most. A reply goes into the thread of the comment it answers.

### post

`post TEXT [-i PICTURE …] [--topic TOPIC]`: up to nine PNG or JPEG
pictures and one topic (圈子). `post TEXT --quote POST` reposts `POST`
with `TEXT` as your words (text only). Jike has no replies as posts:
`--reply-to` is refused; use `comment`.

### edit

Changes one of your posts or reposts the way web.okjike.com's post menu
does. It loads the post first, prints the change (each field as old →
new) and asks (`-y` in scripts).

- TEXT replaces the text. Pictures, the video and a link card stay as they
  are; they cannot change here.
- `--topic TOPIC` moves the post to another topic (圈子, one at most);
  `--no-topics` takes it out of its topic.
- `--visibility public|private` makes the post public or visible only to
  you (仅自己可见); `friends` is refused.
- Only what the post's menu offers changes: Jike marks which posts may be
  edited and which may turn private or public again, and others fail
  with `permission_denied`.

```
atp jike edit https://web.okjike.com/u/someone/post/64a1b2c3d4e5f60718293a4b '改过的文字'
atp jike edit '#1' --topic topic:5523b3e5e4b0c4a8a3a9b1d2
atp jike edit '#1' --visibility private
```

### delete, delete-comment

`delete POST` removes your post or repost; `delete-comment POST COMMENT`
your comment. Both ask before they act.

### download

The pictures of a post, at their original size, and its video, whose
address Jike hands out on request (`/1.0/mediaMeta/interactive`). If
Jike ever asks for its app's media handshake instead, the video fails;
ATP does not perform it.

## Limits

- Stories (日记), private messages, live rooms and the 动态 shortcuts are
  not covered.
- No video uploads, link cards or location on new posts; `edit` changes
  the text, topic and visibility, not the pictures.
- Notifications cannot be filtered by kind or marked read.
- Search returns what Jike's web search returns, in its order; there are
  no filters.
- Without a login, `user-posts` and `topic` see only what the public page
  shows (about ten posts).
- Accounts are identified by `username`, not by Jike's internal user id
  (kept in `extra.user_id`).
