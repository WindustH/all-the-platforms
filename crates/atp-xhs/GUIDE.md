## Logging in

`atp xhs login --browser` is the reliable way. The QR code login works,
but Xiaohongshu often asks the new device for a captcha after the scan.

### Browser or cookie

- `atp xhs login --browser`: log in at https://www.xiaohongshu.com in a
  local browser first; only `xiaohongshu.com` cookies are read.
- `atp xhs login --cookie '...'`: the `Cookie` header of any request to
  `www.xiaohongshu.com` from the browser's developer tools.

The session needs `a1` (the device id) and `web_session`; ATP adds the
`webId` companion of `a1` when it is missing.

### QR code

`atp xhs login --qrcode` (also the default of `atp xhs login`) starts like
a visitor's browser: fresh device cookies and a guest session, then a QR
code to scan with the Xiaohongshu app and confirm there. When the phone has
confirmed but Xiaohongshu wants a captcha before it trusts the new device,
the login fails with `verification_required`; log in at xiaohongshu.com in
a browser and use `--browser` instead.

### Without logging in

Only `read` and `download` work: they fall back to the note's web page,
which also serves visitors. Everything else answers captchas to visitors,
so ATP asks for a login first (`not_authenticated`).

### When the session ends

How long a session lasts is not known. An expired one gives
`not_authenticated` ("not logged in", upstream codes `-100` / `-101`); log
in again.

### Risk control

Xiaohongshu is quick to flag automation. Requests are spaced at least a
second apart; likes, collects, comments, follows, board changes and
private messages wait another 0.8-2 s, and publishing 1-2.5 s. Space
bulk work out further with `--interval`.

A captcha (`verification_required`, HTTP 461 / 471) means stop: ATP first
waits 5 s (doubling with each captcha in a row, up to 30 s), then on a
terminal opens the site in a browser (see `atp guide login`). Pass the
check there and wait before retrying; if the session did not come from
that browser, run `atp xhs login --browser`. Other refusals: `ip_blocked`
(try a different network), `rate_limited`.

### Environment

`ATP_XHS_COOKIE`: a cookie header for one run, never saved.

## References

Most note reads need the note's `xsec_token`, which Xiaohongshu puts in the
links it shows. Use:

- links: `https://www.xiaohongshu.com/explore/<id>?xsec_token=…`,
  `/discovery/item/<id>?…`, `https://xhslink.com/…` short links, or shared
  text containing one;
- `#N` and the `url` of earlier results (they carry the token);
- a bare note id only after it appeared in a list (tokens are cached a day).

Users: the id, a `/user/profile/<id>` link, or a short link.

Chats: the other account's user id (from `chats`) or a profile link;
messages are `<chat>/<message id>`. Live rooms (`listen`): `live:ROOM`, a
`/livestream/<room>` link or the room number. Boards: the id, a
`/board/<id>` link or `#N` after `collections`.

Activities (EVENT): `#N` of the last list, the activity id, its link, or
its exact name.

## Commands

### search

`--sort general|popular|latest|comments|collects`; `--filter` takes one of
`video`, `image` (note type) or `day`, `week`, `half-year` (publication
time).

### hot, feed, channels

`hot --category` and `feed --type recommend|fashion|food|…|video` read a
home-page channel (`hot` defaults to `food`, `feed` to `recommend`);
`channels` lists them with their names.

### suggest, trending

`suggest KEYWORD` lists the search box's completions; `trending` lists what
it offers before you type (猜你想搜, personalized).

### comments, replies

Comments have no sort order. Read comments carry their images in
`extra.pictures`. `replies` starts at the oldest reply.

### collections, favorites

`collections [USER]` lists boards (专辑) and `favorites --folder BOARD`
their notes; few accounts still have boards. Without `--folder`,
`favorites` lists the notes a user collected.

### folder-create, folder-edit, folder-delete

Boards are the favorites folders here. `folder-create NAME [--desc TEXT]
[--private]` makes one as the profile's 专辑 tab does: a name of 12
characters at most, a description of 1000, public unless `--private`.
`folder-edit BOARD [--name …] [--desc …] [--private | --public]` reads the
board first and keeps what is not given. `folder-delete BOARD…` asks
first (`-y` in scripts) and deletes one board per call; the notes in it
stay collected and keep showing in `favorites`. Boards have no cover of
their own (`--cover` is not offered).

### folder-add, folder-move

`folder-add BOARD NOTE…` puts notes into a board and `folder-move FROM TO
NOTE…` moves them there, one call per note (`note/move`, the call the
web's board picker makes after you collect a note). The call, named for
moving notes between boards, names only the board a note goes to; as far
as the web shows, a note sits in one board at a time, so adding it to a
board takes it out of any other, `folder-move` does not check that the
notes were in FROM, and `--copy` is refused. There is no
`folder-remove`: the web has no way to take a note out of a board
without uncollecting it (`unfavorite`). Collect a note before adding it;
whether `folder-add` collects one you have not collected is not known.

### following

`following` lists the accounts you follow from the message page's list of
them, with `extra.mutual` (you follow each other). On your own list, as
the profile's relation panel (关注) offers it:

- `--sort recent|oldest|smart`: latest follow first, earliest first, or
  Xiaohongshu's own mix (综合排序). Rows carry `extra.mutual` and
  `extra.remark` (the name you gave them).
- `--search WORDS` finds the accounts you follow by name through the
  panel's search box (搜索已关注的人).
- `--mutual` lists your mutual follows (互相关注), from the message page's
  list as its share dialog reads it.

`--sort`, `--search` and `--mutual` go one at a time. `following USER`
lists whom someone else follows, latest first, as their relation panel
does; people can hide the list (关注列表不可见) and Xiaohongshu may show
only part of it.

### followers

`followers [USER]` lists who follows an account, latest first, as the
relation panel's 粉丝 tab does: yours (rows carry `extra.mutual`) or,
where they show it, someone else's.

### invalid-follows

`invalid-follows` lists the accounts you follow that the relation panel's
batch manager (批量管理) files under its `invalid` tab, latest follow
first. Which accounts land there (closed or banned ones, going by the
name) and what the page calls the tab come from the server and are not
known. Unfollow them with `unfollow '#1' '#2' …`.

### follow, unfollow

One by one, except that `unfollow` with several users takes them back 50
per call (`followings/batch/unfollow`, as the batch manager does; it
warns that this cannot be undone).

### history, history-count, history-delete, history-clear

`history` is your browsing history (浏览记录) of notes, with
`extra.read_at`; `history-count` says how many notes it holds.
`history-delete NOTE…` removes notes from it in one call (an Action per
note) and `history-clear` empties it; both ask first (`-y` in scripts).
Live rooms in the history are left alone.

### search-saved

`search-saved KEYWORD [--likes]` searches your own collected (or liked)
notes.

### notifications, mark-read

`--type mentions|likes|connections` (`mentions` by default). `mark-read
[--type …]` marks one tab, or all of them, as read.

### chats, messages

`chats` lists your private-message conversations (私信) and
`messages CHAT` reads one, newest first.

### send

`send CHAT TEXT` sends a text private message to the account CHAT names,
over the web's push connection (`apppush-rws.xiaohongshu.com`), as the
message page does, and waits up to 10 s for the receipt. Text only: no
`--attach` or `--reply-to`. When no receipt comes the message may still
have gone out; check `messages CHAT` before sending it again. Not yet
verified against a live account.

### listen

`listen` prints the private messages your account receives as they
arrive; `listen CHAT …` keeps to those conversations. `listen live:ROOM …`
follows live rooms over the same connection: posts of kind `message` (a
comment), `gift` (name × count), `like`, `enter`, `follow` or `notice`,
with `extra.chat` `live:ROOM`; private messages then come only from the
users named alongside. Both need a login (the connection logs in with the
session's token from `celestial/lt`) and are not yet verified against a
live account. A dropped connection is opened again.

### post, delete

`post` publishes an image note: it needs at least one image (`-i`) and takes a
`--title`. `--topic` and `#tags` in the text become topics (each the topic
search's best match, at most 10). No `--reply-to` or `--quote`. `delete`
removes one of your notes.

### comment, like-comment

`comment NOTE "text" [--reply-to COMMENT]`; `like-comment NOTE COMMENT
[--undo]`. Comments are text only: images in comments are an app feature
the web API ignores.

### favorite, unfavorite

Collects notes or uncollects them, one call each: the web sends its
uncollect call one note at a time, though the field is named `note_ids`,
and whether it takes several is not known.

`favorite NOTE… --folder BOARD` collects each note, then puts it into
the board as `folder-add` does; when that second step fails, the note
stays collected outside the board and the error says so. `unfavorite`
refuses `--folder`: uncollecting takes a note out of its board too.

### insights

`atp xhs insights [--days N]`: the creator data center, with 7-day and
30-day windows only (`--days` up to 7 uses the first).

- totals and daily series: `impressions`, `views`, `ctr`,
  `avg_watch_seconds`, `completion_rate`, `likes`, `comments`, `favorites`,
  `shares`, follower gains and losses, `profile_views`, posts published.
- breakdowns: `traffic_source`, `hour_of_day`, `vs_similar_creators`; fan
  portraits (gender, age, city, interest, follow source) from 50 fans.

`atp xhs insights <your note>`: lifetime totals, daily series, audience,
traffic sources and, for videos, `retention`. Others' notes get public
counters. The data center has to be enabled once (it is requested on first
use and shows data from the next day).

### note-stats, active-fans, my-notes

`note-stats` lists your notes with impressions, views, CTR, watch time and
follows; `active-fans` your most engaged fans; `my-notes` your notes.

### events, event, keep-event, join-event

The creator center's activities (活动中心): official campaigns with rewards
for notes about their topic.

- `events [KEYWORD] [--kept] [--latest]` lists the running activities, or
  those whose name, reward or topic contains KEYWORD; `--kept` only the ones
  you kept.
- `event EVENT` shows one: rewards, start and end, topics, links.
- `keep-event EVENT [--undo]` keeps (收藏) an activity or drops it.
- `join-event EVENT [TEXT] --title T -i IMAGE [--topic T]` publishes an
  image note for the activity, as the publish page's 关联活动 does: the
  activity's topics are added to the note and the note is linked to the
  activity. One activity per note.

### download

Images at full resolution (JPEG transcodes of the originals) and the best
video stream, including live photos.

## Limits

ATP signs requests itself, as the web app and the creator center do; when
Xiaohongshu rejects a signature, commands fail with `signature_error`.

`likers` and `reposts` do not exist on Xiaohongshu; who liked your notes
shows in `notifications --type likes`.

The relation panel (`following --sort` and `--search`, others'
`following`, `followers`, `invalid-follows`), the batch unfollow and the
board writes are built from the web app's page bundles and have not been
run against a live account yet. The web shows the relation panel only to
accounts in an experiment (`enable_web_follow_list`); whether its calls
answer other accounts is not known. Some reads are said to want the
creator center's signature (XYW) since about March 2026; ATP signs these
calls as the main site does, and whether that is enough is not known.

The web has no follow groups, remarks to set, way to remove a follower,
or blocking: it sends people to the app to unblock, its message client
holds block calls nothing on the page uses, and there is no list of
blocked accounts. ATP offers none of them.
