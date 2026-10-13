## References

- Videos: `BV1xx411c7mD`, `av170001`, `https://www.bilibili.com/video/BV…`
  (with `?p=N` for a part), `https://b23.tv/…` short links.
- Dynamics: numeric ids (17+ digits), `https://t.bilibili.com/<id>`,
  `https://www.bilibili.com/opus/<id>`.
- Users: the numeric mid (`946974`), `https://space.bilibili.com/<mid>`, or a
  name (looked up through user search).
- Articles (专栏): `cv12345`, `https://www.bilibili.com/read/cv12345`.
- Bangumi, films, documentaries: `ss…` (season), `ep…` (episode), `md…`
  (review page) or their `/bangumi/` links.
- Live rooms: the room number, `live:ROOM` or a `live.bilibili.com/…` link;
  `live` also takes a user (`uid:MID`, `@name`).
- Collections (合集 / 系列): their space link, `MID/ID` (a 合集) or
  `MID/series/ID`.
- Private chats: `dm:MID` from `chats`, a uid or a space link (never a name).
- Reply to a comment with `--reply-to ROOT` or `--reply-to ROOT:PARENT`.

## Logging in

`atp bili login` shows a QR code for the Bilibili app. Most reads work
without an account; subtitles, history, the dynamic feed, favorites of
others, notifications and every write need one.

## Analytics

`atp bili insights [--days N]`: the creator data center. `--days` is
rounded up to Bilibili's windows (yesterday, 7, 30, 90 days, or all).

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

`atp bili video-stats` lists your videos with those numbers side by side
(one row per video with `-f csv`).

## Search and discovery

- `search Q --filter video|article|bangumi|film|live` (videos by default),
  `search Q -t user`, `search Q -t topic` (话题; ids work with `topic`).
- `trending` (热搜), `weekly [NUMBER]` (每周必看), `precious` (入站必刷),
  `rank [--region R]`, `topic TOPIC [--sort hot|new]`.
- `bangumi SEASON [--info]` lists episodes (or the season's rating and
  counters), `timeline [--drama]` what airs this week, `my-bangumi [USER]
  [--drama]` what someone follows, `bangumi-follow SEASON [--undo]`.
  Bangumi and films are metadata only: `download` keeps to ordinary videos.
- `lists USER` shows a user's 合集 / 系列 and `list LIST` their videos;
  `articles USER`, `coined USER`, `liked USER` (when they make them public).
- `read` adds your own relation to a video under `extra.mine` (liked, coins,
  favorited, following) and `user` the lifetime plays of their uploads.

## Chats and live rooms

- Private messages (私信): `chats` lists conversations (`dm:MID`),
  `messages CHAT` their history, `send CHAT TEXT` replies (text only);
  `unread` includes `dm_followed` / `dm_others`.
- Live: `live ROOM` shows a room, `live-following` the rooms of people you
  follow, `messages live:ROOM` its latest danmaku, `send live:ROOM TEXT`
  sends danmaku, and `listen live:ROOM …` streams danmaku, super chats,
  gifts, memberships and live start / end as they happen (names are hidden
  from logged-out viewers).

## Library

- History: `history-search Q`, `history-delete VIDEO`, `history-pause
  [--undo]`, `history-clear --yes`.
- Watch later: `watch-later`, `later-add VIDEO`, `later-remove VIDEO`,
  `later-clear [--watched] [--yes]`.
- Folders: `folder-create TITLE [--intro] [--private]`, `folder-edit`,
  `folder-clean` (drops unavailable videos), `folder-delete --yes`,
  `saved-folders` (folders and 合集 you saved), `save FOLDER|LIST [--undo]`.
- Follow groups: `groups`, `group GROUP`, `group-create`, `group-rename`,
  `group-delete --yes`, `group-assign USER… --to GROUP…`, `special USER
  [--undo]` (特别关注); `blacklist`, `block USER [--undo]`,
  `shared-following USER`.

## Interactions

- `likers` / `reposts` work on dynamics, and on videos through the dynamic
  that announced them.
- `coin VIDEO [-n 1|2] [--like]` and `triple VIDEO` spend coins: they cannot
  be undone.
- `subtitle VIDEO [--lang L]` and `danmaku VIDEO` return timed text;
  `summary VIDEO` Bilibili's AI summary.
- `post` publishes a dynamic (text and images, one topic id with `--topic`,
  `--quote` to repost); `pin DYNAMIC [--undo]` pins it on your space.
- `like`, `favorite` and `comments` work on articles too.
- `comment-like POST COMMENT [--dislike] [--undo]`, `comment-pin POST
  COMMENT [--undo]` (under your own posts), `danmaku-send VIDEO TEXT --at
  SECS [--color HEX] [--mode scroll|top|bottom]`.

## Downloads

Video and audio come as separate DASH streams (best quality your account
may see) and are merged with ffmpeg; `--audio-only` keeps the audio.

## Limits

- The data center no longer reports traffic sources (recommend, search ...).
- Per-video trends cover the first and the last 30 days after publishing.
- Reposts carry only a display date, so `--since` cannot filter them.
