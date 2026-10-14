## Logging in

### QR code

`atp bili login` shows a QR code of Bilibili's web login. Scan it with the
Bilibili app and confirm the login on the phone. The session keeps the
cookies `SESSDATA`, `bili_jct`, `DedeUserID` and `DedeUserID__ckMd5`, and
the `refresh_token` the login hands over.

### Browser or cookie

- `atp bili login --browser` reads the cookies of a browser logged in at
  bilibili.com.
- `atp bili login --cookie '...'`: copy the `Cookie` header of a request to
  www.bilibili.com or api.bilibili.com. It must hold `SESSDATA` and
  `bili_jct`; `bili_jct` is the CSRF value every write sends.
  `DedeUserID` (your uid) saves a lookup.
- `ATP_BILI_COOKIE='...'` passes such a header for one run; it is never
  saved.

There are no other environment variables and no tokens.

### Without an account

Most reads work without an account: search, hot, rankings, `read`,
`comments`, `user`, `user-posts`, a user's lists and articles, bangumi,
live rooms and `download`. Subtitles (most tracks), history, the dynamic
feed, favorites of others, followers and following, notifications, watch
later, chats, `live-following`, account insights and every write need one.
`listen` works logged out, but Bilibili hides the senders' names.

Logged out, ATP fetches the device cookies a browser gets on its first
visit (`buvid3`, `buvid4`, `b_nut`); without them many anonymous reads fail
with -352 or -412.

### When the session ends

How long a session lasts is not known. ATP saves the `refresh_token` but
does not refresh the session itself. When it ends, commands fail with
`not_authenticated` (Bilibili codes -101, -111, -2, 61000) and the hint to
run `atp bili login` again.

### Risk control

- Captchas (codes -352, 12015, or a "success" carrying only a `v_voucher`)
  are `verification_required`. On a terminal ATP opens bilibili.com in a
  browser to pass the check (see `atp guide login`); otherwise wait a while
  or log in.
- HTTP 412 and codes -412, -509, -799, 34004, 12051, 12052 are
  `rate_limited`: wait a while before retrying, or log in.
- ATP keeps at least 300 ms between requests and waits a random 0.4 to
  1.2 s before every write.
- ATP speaks the website's own API, the way the web client does (WBI
  signatures, device cookies); it is not an official API.

## References

- Videos: `BV1xx411c7mD`, `av170001`, `https://www.bilibili.com/video/BV…`
  (with `?p=N` for a part), `https://b23.tv/…` short links.
- Dynamics: numeric ids (17+ digits), `https://t.bilibili.com/<id>`,
  `https://www.bilibili.com/opus/<id>`.
- Users: the numeric mid (`946974`), `uid:MID`,
  `https://space.bilibili.com/<mid>`, or a name (looked up through user
  search, best match).
- Articles (专栏): `cv12345`, `https://www.bilibili.com/read/cv12345`.
- Bangumi, films, documentaries: `ss…` (season), `ep…` (episode), `md…`
  (review page) or their `/bangumi/` links.
- Live rooms: the room number, `live:ROOM` or a `live.bilibili.com/…` link;
  `live` also takes a user (`uid:MID`, `@name`).
- Collections (合集 / 系列): their space link, `MID/ID` (a 合集) or
  `MID/series/ID`.
- Favorites folders: the folder id, a `…/favlist?fid=<id>` link or `#N`
  after `collections`.
- Follow groups: the group id or its exact name.
- Topics (话题): the id or a link with `topic_id=`.
- Private chats: `dm:MID` from `chats`, a uid or a space link (never a name).
- Comments: `ROOT`, or `ROOT:PARENT` for a reply inside a thread; reply to
  a comment with `--reply-to ROOT` or `--reply-to ROOT:PARENT`.

## Commands

### search

- `--filter video|article|bangumi|film|live` (videos by default).
- `search Q -t user`, `search Q -t topic` (话题; ids work with `topic`).

### trending, weekly, precious, rank, topic

`trending` (热搜), `weekly [NUMBER]` (每周必看, the latest issue by
default), `precious` (入站必刷), `rank [--region R]` (the site, or one region
of `hot --category`), `topic TOPIC [--sort hot|new]`.

### read

Adds your own relation to a video under `extra.mine` (liked, coins,
favorited, following).

### user

Includes the lifetime plays of the user's uploads.

### followers, following

Need a login, also for other users. Bilibili caps others' lists at 5 pages.

### comments, replies

`comments` works on videos, dynamics and articles. `--sort hot` (default)
orders by likes; `--sort time` is newest first, and logged-out visitors get
only the first few. `replies` puts the pinned reply first (`extra.pinned`).

### likers, reposts

Work on dynamics, and on videos and articles through the dynamic that
announced them.

### like, unlike, favorite, unfavorite

Work on videos, dynamics and articles. `favorite VIDEO --folder F` uses one
of your folders; without it the default folder.

### collections, favorites

`collections [USER]` lists favorites folders; `favorites [USER] [--folder
F]` the videos of a folder (the default folder when omitted).

### post

Publishes a dynamic: text and images, one topic id with `--topic`, `--quote
DYNAMIC` to repost (a repost cannot carry images). Replies go through
`comment`.

### delete

Deletes dynamics only; videos and articles are not deleted here.

### pin

`pin DYNAMIC [--undo]` pins one of your dynamics on your space.

### insights

`atp bili insights [--days N]`: the creator data center. `--days` is
rounded up to Bilibili's windows (yesterday, 7, 30, 90 days, or all). Days
follow Beijing time; a day's data appears the next noon.

- totals and daily series: `views`, `profile_visitors`, `likes`,
  `favorites`, `coins`, `comments`, `danmaku`, `shares`, `followers`,
  `new_followers`, `lost_followers`, `net_followers`, `active_followers`;
  `extra.previous_period` holds the same numbers for the period before.
- breakdowns: `device`, `viewer_type` (followers vs others), `follow_source`,
  `video_last_day` (plays per video), and a follower portrait (gender, age,
  region, interest, active hours) once Bilibili builds one.

`atp bili insights <your video>`: lifetime counters, average watch
seconds and share watched, 3-second bounce, interaction, follow-conversion
and non-follower rates, rank among similar videos (`extra.beats_similar`),
daily trends, audience breakdowns and a `retention` curve. Other videos get
their public counters plus the number watching now.

### video-stats

Lists your videos with the `insights` numbers side by side (one row per
video with `-f csv`); alias `my-videos`.

### download

Video and audio come as separate DASH streams (best quality your account
may see) and are merged with ffmpeg; `--audio-only` keeps the audio.
Bangumi and films are metadata only: `download` keeps to ordinary videos.

### coin, triple

`coin VIDEO [-n 1|2] [--like]` and `triple VIDEO` (like, coin and favorite
at once) spend coins: they cannot be undone.

### subtitle, danmaku, summary

`subtitle VIDEO [--lang L]` and `danmaku VIDEO` return timed text; most
subtitle tracks are served to logged-in accounts only. `summary VIDEO` is
Bilibili's AI summary and outline.

### bangumi, bangumi-follow, my-bangumi, timeline

`bangumi SEASON [--info]` lists episodes (or the season's rating and
counters), `timeline [--drama]` what airs this week, `my-bangumi [USER]
[--drama]` what someone follows, `bangumi-follow SEASON [--undo]`.

### lists, list, articles, coined, liked

`lists USER` shows a user's 合集 / 系列 and `list LIST` their videos;
`articles USER`, `coined USER`, `liked USER` (when they make them public).

### chats, messages, send

- Private messages (私信): `chats` lists conversations (`dm:MID`; Bilibili
  chats are flat, no PARENT), `messages CHAT` their history, `send CHAT
  TEXT` replies (text only: no `--attach`, no `--reply-to`).
- Live rooms: `messages live:ROOM` shows its latest danmaku, `send
  live:ROOM TEXT` sends danmaku.

### unread

Includes the private message counters `dm_followed`, `dm_others` and
`dm_groups`.

### live, live-following

`live ROOM` shows a room, `live-following` the rooms of people you follow.

### listen

`listen live:ROOM …` streams danmaku, super chats, gifts, memberships and
live start / end as they happen (names are hidden from logged-out viewers).

### history-search, history-delete, history-pause, history-clear

`history-search Q`, `history-delete VIDEO`, `history-pause [--undo]`,
`history-clear --yes`.

### watch-later, later-add, later-remove, later-clear

`watch-later`, `later-add VIDEO`, `later-remove VIDEO`, `later-clear
[--watched] [--yes]` (`--yes` only when emptying the whole list).

### folder-create, folder-edit, folder-clean, folder-delete, saved-folders, save

`folder-create TITLE [--intro] [--private]`, `folder-edit`, `folder-clean`
(drops unavailable videos), `folder-delete --yes`, `saved-folders` (folders
and 合集 you saved), `save FOLDER|LIST [--undo]`.

### groups, group, group-create, group-rename, group-delete, group-assign, special

`groups`, `group GROUP`, `group-create`, `group-rename`, `group-delete
--yes` (its people move to the default group), `group-assign USER… --to
GROUP…` (replaces their groups), `special USER [--undo]` (特别关注).

### blacklist, block, shared-following

`blacklist`, `block USER [--undo]`, `shared-following USER`.

### comment-like, comment-pin, danmaku-send

`comment-like POST COMMENT [--dislike] [--undo]`, `comment-pin POST
COMMENT [--undo]` (under your own posts), `danmaku-send VIDEO TEXT --at
SECS [--color HEX] [--mode scroll|top|bottom]`.

## Limits

- The data center no longer reports traffic sources (recommend, search ...).
- Per-video trends cover the first and the last 30 days after publishing.
- Reposts carry only a display date, so `--since` cannot filter them.
