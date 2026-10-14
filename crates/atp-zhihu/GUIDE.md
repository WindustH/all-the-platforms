## Logging in

### QR code

`atp zhihu login` (the default) shows a QR code; scan it with the Zhihu app
and confirm there. ATP starts from an empty cookie jar and checks the
network once before the code is shown, so a flagged network fails before
anyone scans. When Zhihu reports risk control during the scan, the login
stops with `verification_required`: pass the check in a browser (its link
is in the hint), retry later or through another network (`--proxy`), or
use one of the ways below.

### Browser or cookie

- `atp zhihu login --browser`: log in at https://www.zhihu.com in a local
  browser first; only `zhihu.com` cookies are read.
- `atp zhihu login --cookie '...'`: the `Cookie` header of any request to
  `www.zhihu.com` from the browser's developer tools.

Either way the session needs `z_c0`. ATP fetches the helper cookies
`_xsrf` (which writes need) and `d_c0` when they are missing.

### Without logging in

Only `hot` (through the app's API) and `daily` (Zhihu Daily) work. Zhihu
refuses anonymous web API requests that lack its `x-zse-96` signature
(`10003`) and flags networks that keep trying (`40352`), so ATP does not
try: every other command fails with `not_authenticated` ("Zhihu serves this
to logged-in sessions only").

### When the session ends

How long a session lasts is not known. When it ends, commands fail with
`not_authenticated`; log in again.

### Risk control

Error codes `40352` / `40362` and the "unhuman" check page give
`verification_required`. On a terminal ATP opens the check in a browser
(see `atp guide login`); elsewhere the link is in the hint. Requests are
spaced at least 0.6 s apart, and each write waits another 0.4-1.2 s.

### Environment

`ATP_ZHIHU_COOKIE`: a cookie header for one run, never saved.

## References

- Links: `https://www.zhihu.com/question/<q>`, `…/question/<q>/answer/<a>`,
  `https://www.zhihu.com/answer/<a>`, `https://zhuanlan.zhihu.com/p/<id>`
  (article), `https://www.zhihu.com/pin/<id>` (pin, 想法),
  `https://www.zhihu.com/zvideo/<id>` (video).
- Typed ids: `q:<id>`, `a:<id>`, `article:<id>`, `p:<id>` (pin), `v:<id>`
  (video); the long forms `question:`, `answer:`, `pin:`, `zvideo:` work
  too. A bare number is an answer (a question where only questions fit).
- Users: the url_token (`zhang-jia-wei`), `@token`, or a `/people/` or
  `/org/` link.
- Topics: the id, a `/topic/<id>` link, or `#N` after a topic list.
- Columns: the id, `https://zhuanlan.zhihu.com/<id>`,
  `https://www.zhihu.com/column/<id>`, or `#N` after a column list.
- Favorites folders: the id, or `#N` after `collections`.

## Commands

### hot

The hot list (热榜), up to 50 entries. It has no categories any more: Zhihu
serves the full list for every category name.

### feed

The default is 推荐; `--type follow` is the 关注 feed (who did what is in
`extra.action_text` and `extra.actor`).

### read

Questions are read from their pages: their API endpoint needs the
`x-zse-96` signature.

### comments

`--sort score` (hot first, the default) or `ts` (newest first).

### likers

Upvoters of answers, articles and pins, newest first.

### user-posts

A user's answers; `user-articles`, `user-pins` and the other `user-*`
commands list the rest.

### collections, favorites

`collections [USER]` lists favorites folders (收藏夹). `favorites [USER]
[--folder ID]` shows one folder's items: the first folder when none is
given.

### history

What you read, newest first (`extra.read_at`).

### notifications

`--type comment|like|favorite|follow|message`: the app's timelines;
`message` lists private-message threads. `all` (default) is the web's
recent list.

### like, unlike

Upvotes (赞同) an answer, or likes an article or a pin; `unlike` goes back to
neutral. Questions cannot be liked (use `follow-question`), nor videos.

### vote

`vote ANSWER up|down|neutral` (反对 is `down`). Articles and pins take `up`
or `neutral` only.

### favorite, unfavorite

Saves to the account's first folder, or to `--folder ID`. Questions cannot
be saved (use `follow-question`).

### like-comment, block

`like-comment COMMENT [--undo]`; `block USER [--undo]` (屏蔽).

### follow, unfollow

Users only; `follow-question QUESTION [--undo]` follows a question.

### post

Publishes a pin (想法) with an optional `--title` and images. Pins take no
`--reply-to`, `--quote` or `--topic`; use `ask` / `article` for topics.

### answers

`answers QUESTION [--sort default|created]`: `default` orders by votes.

### ask

`ask TITLE [-d DETAIL] [-t TOPIC] [-i IMAGE]`; topics are topic ids.

### answer, article

`answer QUESTION BODY [-m] [-i IMAGE]` answers a question and `article
TITLE BODY [-m] [-t TOPIC] [-i IMAGE]` publishes a column article; `-` as
BODY reads it from stdin: `atp zhihu answer q:123 - -m < answer.md`.

With `-m / --markdown` the body is read as Markdown: `#` / `##` headings
(deeper levels become bold), `-` and `1.` lists, `>` quotes, fenced code,
`|` tables, `---` rules, `**bold**`, `` `code` ``, `[text](url)` links, an
image alone on its line (`![caption](path)`, uploaded and placed there), and
a link alone on its line with the title `"card"` (`[text](url "card")`) as a
link card. Without it, every line becomes a paragraph.

### delete

Removes your own pin, article, question or answer (not videos).

### insights

`atp zhihu insights [--days N]`: the creator center (创作中心), for the last
N days up to today in China time.

- totals and daily series: `views`, `impressions`, `ctr`, `completion_rate`,
  `likes` (赞同), `hearts` (喜欢), `reactions`, `comments`, `favorites`,
  `shares`, `reposts`, follower gains and losses, `profile_visitors`.
- breakdowns: `traffic_source`, reader `gender` / `age` / `region` (lifetime),
  `content_type`, and a follower portrait once Zhihu builds one.

`atp zhihu insights <your answer, article or pin>`: the same for one post
plus followers it brought. Others' posts get public counters.

### creations

`creations -t answer|article|pin` lists your posts with their lifetime
numbers.

### download

Videos (`zvideo`) and video pins come in their best stream.

### activities, user-questions, user-videos, user-columns, user-following

`activities [USER]` (动态), `user-questions`, `user-videos`, `user-columns`
and `user-following [USER] -t questions|topics|columns|folders`; without
USER they show your own.

### topic

`topic ID` shows a topic; `--essence` lists its essence posts. Other topic
feeds are not offered: their API endpoints need the `x-zse-96` signature.

### question-followers, topic-followers, column

`question-followers QUESTION`, `topic-followers TOPIC`, `column COLUMN
[--info]` (articles of a 专栏, or the column itself).

### hot-search, daily

`hot-search` lists trending searches (热搜); `daily [--date YYYYMMDD]`
reads Zhihu Daily (知乎日报), no login needed.

### create-folder, delete-folder

`create-folder TITLE [-d TEXT] [--private]` and `delete-folder FOLDER -y`
manage favorites folders (`collections` lists them). The default folder
cannot be deleted.

## Limits

Zhihu signs its web requests with `x-zse-93` / `x-zse-96`; ATP does not.
Logged-in sessions work without it, except where noted (questions, topic
feeds); a `10003` answer to a logged-in session gives `signature_error`.

Reposts have no public list, so there is no `reposts`.
