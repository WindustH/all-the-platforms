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
feed, favorites of others, followers and following, follow groups, the
blacklist, notifications, watch later, chats, `live-following`, account
insights and every write need one.
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
  1.2 s before every write, also between the users or posts of one
  command.
- Bilibili caps how many accounts one follows (22009) and blocks (22008);
  bulk follows, blocks and removals are what risk control watches most:
  space them out with `--interval`.
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
- What a favorites folder holds (`folder-remove`, `folder-move`): videos,
  audio tracks (`au123`, `https://www.bilibili.com/audio/au123`) and
  collections (`season:ID`, a 合集's space link or `MID/ID`), or `#N`
  after `favorites --folder F`.
- Follow groups: the group id, its exact name or `#N` after
  `follow-groups`. `-10` is the special-follow list (特别关注), `0` the
  default group (默认分组) of everyone in no other group.
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

- `following --sort recent` (the default, latest follow first) or
  `--sort visited` (most visited first, 最常访问); `visited` orders only
  your own list.
- `following [USER] --search WORDS` finds followings by name (Bilibili's
  followings search, in follow order; no `--sort`).
- `following --mutual` lists your mutual follows (互相关注), all in one
  call; it takes no USER, `--sort` or `--search`.
- On your own `following`, rows carry `extra.mutual`, `extra.groups`
  (follow group ids, `[]` for the default group), `extra.special`
  (特别关注) and `extra.followed_at`; on your own `followers`,
  `extra.mutual` (whether you follow back) and `extra.followed_at`.
  Others' lists carry `followed_at` only.

### follow, unfollow

Following several users goes through Bilibili's batch call, 50 users per
call: all of them are looked up first (a name that finds nobody stops the
command before anything changes), and users the call refuses come back
with `ok: false`. Unfollowing several goes one by one.

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
F]` what a folder holds (the default folder when omitted), newest saved
first, with `extra.favorited_at` (unix seconds). Besides videos a folder
holds audio tracks (`kind: audio`, id `au…`) and collections of videos
(合集, `kind: season`, id `season:ID`, linked to the owner's space).

### post

Publishes a dynamic: text and images, one topic id with `--topic`, `--quote
DYNAMIC` to repost (a repost cannot carry images). Replies go through
`comment`.

### edit

Changes one of your videos or dynamics. It loads the post first, prints
the change (each field as old → new) and asks (`-y` in scripts).
Articles (专栏) are not edited.

Videos (`BV…`, `av…`) change the way the creator center's edit page
changes them:

- TEXT replaces the description (简介, at most 2000 characters).
- `--title` replaces the title (at most 80 characters).
- `--topic NAME` (repeatable) replaces the tags, 10 at most; a topic the
  video joined (参与话题) keeps its tag in front. A video keeps at least
  one tag, so `--no-topics` is refused.
- `--cover IMAGE` uploads a new cover (a 16:9 JPEG or PNG); the 4:3 crop
  is cleared.
- `--visibility public|private`; `friends` is refused.
- The edit page sends the whole record back, so every other field is
  copied from what was loaded. Joint submissions (联合投稿), videos for
  chargers (充电专属), interactive videos and videos with a commercial
  order are refused, and so is a new cover while a cover test runs.
- Bilibili reviews a changed video again. While a video is reviewed, or
  before it first opens, Bilibili refuses the edit and says why.

Dynamics (`t.bilibili.com/ID`) change the way their editor changes them:

- TEXT replaces the text; @-mentions in it are plain text. Pictures stay.
  A dynamic with a lottery or a vote keeps its text (a new one would drop
  them).
- `--title` sets the title, an empty one removes it; reposts have none.
- `--topic ID` sets the one topic (a numeric topic id, as on `post`);
  `--no-topics` removes it.
- `--cover` and `--visibility` are refused.
- Only dynamics Bilibili offers to edit change: your own, not while they
  are reviewed, and not with a commercial card. The summary shows the
  edits left and Bilibili's warning when it gives them.

```
atp bili edit BV1xx411c7mD --title '新标题' --topic 编程 --topic Rust
atp bili edit BV1xx411c7mD --visibility private
atp bili edit https://t.bilibili.com/1012345678901234567 '改过的文字'
```

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

### history-search, history-pause

`history-search Q` searches the titles of your watch history;
`history-pause [--undo]` stops recording it (and starts again).

### history-delete, history-clear

Videos only. `history-delete VIDEO…` removes them from the watch history
one call each; `history-clear` erases all of it. Both ask first (`-y` in
scripts).

### watch-later, later-add, later-remove, later-clear

Videos only. `watch-later` reads the whole list in one request (`-n`
cuts it, `--cursor` goes on); `later-add VIDEO…` and `later-remove VIDEO…`
go one call each. `later-clear` empties the list after asking (`-y` in
scripts); `later-clear --watched` drops only the videos already watched.

### folder-create, folder-edit, folder-delete

- `folder-create NAME [--desc TEXT] [--private]`; the result carries the
  new folder's id and link.
- `folder-edit FOLDER` changes `--name`, `--desc`, `--private` /
  `--public` and `--cover IMAGE`; Bilibili's edit takes the whole folder,
  so what is not given is sent back as it was. A new cover is uploaded
  first; Bilibili reviews covers.
- `folder-delete FOLDER… [-y]` deletes the folders with what they hold,
  20 folders per call. The default folder cannot be deleted.

### folder-add, folder-remove, folder-move

- `folder-add FOLDER VIDEO…` saves videos, one call each (as
  `favorite --folder` does); audio and 合集 are not added here.
- `folder-remove FOLDER POST…` takes videos, audio (`au…`) and 合集
  (`season:ID`) out, 20 per call.
- `folder-move FROM TO POST… [--copy]` uses Bilibili's own move and copy,
  20 per call; both folders must be yours.
- POST arguments are all read before anything changes: one that is not a
  video, `au…` or 合集 stops the command.

### folder-clean, saved-folders, save

`folder-clean FOLDER` drops the videos that became unavailable,
`saved-folders` lists the folders and 合集 you saved from others, `save
FOLDER|LIST [--undo]` saves one or drops it.

### follow-groups, follow-group, follow-group-create, follow-group-rename, follow-group-delete, follow-group-add, follow-group-remove

- `follow-groups` lists your groups with the special-follow list (`-10`)
  and the default group (`0`) among them.
- `follow-group GROUP [--sort recent|visited]`; its rows carry no
  relation details beyond `followed_at`.
- `follow-group-delete GROUP [-y]` asks first; its people move to the
  default group.
- `follow-group-add` / `follow-group-remove GROUP USER…`: Bilibili sets a
  user's groups all at once, so ATP reads each user's groups first and
  sends them back with GROUP added or taken out, one user at a time; a
  special follow stays. A user taken out of their last group lands in the
  default group, and `0` itself is refused. Users you do not follow are
  refused by Bilibili.

### special

`special USER [--undo]` marks a special follow (特别关注), the `-10`
group.

### block, unblock, blocked, remove-follower

- `block USER…`: several users go through Bilibili's batch call, 50 per
  call; `unblock` goes one by one.
- `blocked` is your blacklist (黑名单), with `extra.blocked_at`.
- `remove-follower USER… [-y]` (移除粉丝) asks first; one call per user.
- Follows, blocks and removals send the fields the space page sends with
  them.

### shared-following

`shared-following USER` lists the people both you and USER follow.

### comment-like, comment-pin, danmaku-send

`comment-like POST COMMENT [--dislike] [--undo]`, `comment-pin POST
COMMENT [--undo]` (under your own posts), `danmaku-send VIDEO TEXT --at
SECS [--color HEX] [--mode scroll|top|bottom]`.

## Limits

- The data center no longer reports traffic sources (recommend, search ...).
- Per-video trends cover the first and the last 30 days after publishing.
- Reposts carry only a display date, so `--since` cannot filter them.
- `following --sort visited` and `--mutual` exist for your own account
  only.
- How many users or items one batch call takes beyond what ATP sends (50
  users, 20 folder items) is not known.
