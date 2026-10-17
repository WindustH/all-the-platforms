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
session. The writes the web client guards with it need that key, so they
work only on sessions made by these two logins: likes and taking several
likes back, saving a post and unsaving one, comments, follows, approving
follow requests, creating, changing and deleting folders and moving posts
in and out of them, saving mixes, short dramas and music, publishing,
editing and deleting. Whether the passport issues that ticket to ATP's
logins has not been confirmed; without it, these writes fail with
`not_authenticated` ("this session cannot sign writes").

The web client sends the other writes without the key, and so does ATP:
`remove-follower`, `block` / `unblock`, `unfavorite` and `save --undo` /
`save-drama --undo` with several at once, watch later (`later-add`,
`later-remove`, `later-clear`) and `history-clear`. They need a logged-in
session of any kind, `--browser` and `--cookie` included.

### Browser or cookie

Log in on douyin.com in a browser, then:

- `atp douyin login --browser` reads that browser's cookies.
- `atp douyin login --cookie '...'`: copy the `Cookie` header of a request
  to www.douyin.com. It must hold `sessionid`; the `UIFID` cookie a browser
  receives is needed for most reads (below).
- `ATP_DOUYIN_COOKIE='...'` passes such a header for one run; it is never
  saved.

Such sessions read everything a login allows but cannot make the writes
guarded by the write key (above): the browser keeps its key to itself.

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
- Likes, follows, saves and comments also carry device traits
  (`x-tt-session-dtrait`, from a captured Chrome profile); writes sent as
  POST carry the secsdk CSRF token, and ATP waits a random 2 to 6 s
  before each write. They are untested against the live site: another client
  that reproduces them reports likes sent without device traits answered
  with HTTP 403 and the session logged out. Try them on an account you can
  afford to log in again.
- When an answer asks for a captcha (`x-vc-bdturing-parameters`) or, on a
  write, an identity check (`x-tt-verify-passport-decision`), the command
  stops with `verification_required`; on a terminal ATP opens douyin.com
  in your browser. Whether passing a check there clears ATP's next call
  is not known: if it does not, do the same thing once on douyin.com and
  retry later.
- Publishing and deleting go to creator.douyin.com like its web uploader:
  `a_bogus` with the creator page's ids, the secsdk CSRF token, and the
  ticket-guard signature its consumer SDK makes (an HMAC under a key
  agreed with Douyin's built-in server key, over the passport's clock).
  When Douyin wants an identity check (二次验证), a captcha, or answers
  with nothing, the command stops with `verification_required` and the
  upload page; ATP does not open it for you, since passing a check there
  does not clear the call ATP made. Publish once on creator.douyin.com in
  the browser (pass the check it asks for), then retry.

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

### following

`following [USER]` lists whom a user follows, latest follow first. Rows
carry `extra.mutual` (you follow each other), `extra.remark` (the name you
gave them) and `extra.special` (a special follow, 特别关注) where Douyin
tells them. On your own list:

- `--sort recent|oldest|smart`: latest follow first (the default),
  earliest first, or Douyin's own mix (综合排序), as the following page
  offers them.
- `--search WORDS` finds the accounts you follow by name or Douyin id
  through the page's search box (Douyin's suggestions, at most 100).
- `--mutual` lists your mutual follows (互相关注), from the relations the
  share panel loads.

`--sort`, `--search` and `--mutual` go one at a time.

### remove-follower, block, unblock

One user per call. Douyin's web has no list of the accounts you block,
so there is no `blocked`.

### approve-follower

`approve-follower USER…` lets people who asked to follow your private
account (关注请求) follow it, one per call. Needs ATP's own login.

### likes

`likes [USER]`: posts a user liked (public likes only); yours when USER is
omitted.

### collections, favorites

Your own account only. `collections` lists your folders of saved posts
(收藏夹); `favorites [--folder ID]` your saved posts, all or those of one
folder.

### folder-create, folder-edit, folder-delete

`folder-create NAME [--private]` and `folder-edit FOLDER [--name NAME]
[--private | --public]` work like the 收藏夹 tab: names of 15 characters
at most; folders have no description or cover. `folder-edit` without
`--private` / `--public` keeps the folder's privacy (read from
`collections`). `folder-delete` takes one folder per call; what becomes
of the posts in it is not known. FOLDER is an id or `#N` after
`collections`. Needs ATP's own login.

### folder-add, folder-remove, folder-move

`folder-add FOLDER POST…` puts posts into a folder and `folder-remove`
takes them out, 100 per call. `folder-move FROM TO POST…` adds to TO,
then removes from FROM (`--copy` keeps them in FROM). Whether a post must
be saved first, whether `folder-remove` also unsaves it, and whether
adding to one folder takes a post out of another (the call is named
`collects/video/move`) are not known. Needs ATP's own login.

### saved-mixes, saved-dramas, saved-music, save, save-drama, save-music

The other tabs of 收藏: `saved-mixes` lists the mixes (合集) you saved,
`saved-dramas` the short dramas (短剧) and `saved-music` the music.
`save MIX…`, `save-drama DRAMA…` and `save-music MUSIC…` save them and
with `--undo` unsave them; several mixes or dramas unsaved together go 50
per call (no write key needed), the rest one by one. MIX is an id, a
douyin.com/collection/… link or `#N`; DRAMA an id or `#N` after
`saved-dramas`; MUSIC an id, a douyin.com/music/… link or `#N` after
`saved-music`.

### history, history-clear

`history` is what you watched on this account. `history-clear` erases
all of it after asking (`-y` in scripts); the web offers no way to remove
single posts.

### watch-later, later-add, later-remove, later-clear

Your watch-later list (稍后再看): `watch-later` lists it, `later-add
POST…` adds posts (one call each), `later-remove POST…` takes them out
(50 per call), `later-clear` empties it after asking (`-y` in scripts)
and `later-clear --watched` drops only what you watched.

### notifications

The notice center (互动消息) of your account:
`--type all|follower|mention|comment|like|danmaku`.

### like, unlike, favorite, unfavorite

Like posts or save them to your favorites (收藏), or undo either. Several
posts go one by one, except that `unlike` with several takes them back 50
per call (`cancel/item/digg/multi`, as the likes tab does) and
`unfavorite` with several unsaves them 50 per call
(`aweme/cancel/collect/multi`, as the 收藏 tab does); posts Douyin names
as not done there fail. Saving into a folder (`--folder`) is not
offered: save the post, then `folder-add`. Needs ATP's own login (see
Logging in), except `unfavorite` with several posts.

### comment, delete-comment

`comment POST TEXT` comments on a post; `--reply-to COMMENT` answers one of
its comments (the id `comments` lists). Comments are text only.
`delete-comment POST COMMENT` asks first (`-y` in scripts). Both need
ATP's own login; comments are signed with its key.

### follow, unfollow

Follow or unfollow users (a `sec_uid`, a profile link or `@douyin_id`),
one by one. Needs ATP's own login.

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

### post

Publishes through the creator platform (creator.douyin.com) the way its
web uploader does. The files are uploaded first (videos in parts, with
progress; a video upload that stopped continues when the same command
runs again within two hours), then the work is created. Before anything
is uploaded ATP prints what it is about to publish (format, title,
visibility, the AI label, files and sizes) and asks; scripts pass `-y`.
The answer is the new work's id (`post` → `id`), which `delete` and
`read` take.

One of three formats per post:

- **Video**: `--video FILE`, an optional `--title` (30 characters at
  most) and TEXT (the description, 1000 at most). Without `--cover`
  Douyin's first frame is the cover; `--cover IMAGE` uploads one.
- **Image post (图文)**: `-i IMAGE` (repeatable, in order; the first is
  the cover), `--title` (20 at most), TEXT, and `--music ID` for
  background music (ids from `atp douyin musics`).
- **Article (文章)**: `--article FILE`, a Markdown or plain-text file sent
  as the body (100 characters at least; pictures inside it are not
  uploaded, so `![...](...)` is refused), `--title` (required, 30 at
  most), `--cover IMAGE` (required), TEXT as the summary (30 at most),
  and `--music ID`.

Options for all three:

- `--topic NAME` (or `--tag`, repeatable) adds a hashtag after the text.
  Videos and image posts take any name, as typed in the editor; an
  article's topics must be ones Douyin suggests for that exact name.
- `--visibility public|friends|private` (default public; `private` is
  仅自己可见). Posts in a collection must be public.
- `--ai` sets Douyin's 「内容由AI生成」 declaration (自主声明), which China's
  rules on labelling AI-generated content require for AI-made or
  AI-narrated work: the item carries `declare.user_declare_info` =
  `{"choose_value":"aigc"}`. Videos and image posts only; the article
  form has no declaration.
- `--schedule WHEN` publishes later, 2 hours to 14 days ahead (`+3h`,
  `2026-10-20 20:00` in local time, or an RFC 3339 time).
- `--collection NAME|ID` adds the post to one of your collections (合集)
  as its next episode (`atp douyin mixes` lists them).

```
atp douyin post --video ep01.mp4 --cover ep01.jpg --title '『操作系统 Ep.01』绪论' --ai --visibility private
atp douyin post -i a.jpg -i b.jpg -i c.jpg --title 测试 '正文' --topic 摄影
atp douyin post --article notes.md --title 读书笔记 --cover head.jpg '一句话摘要'
```

### edit

Changes one of your published videos or image posts the way the creator
platform's edit page (作品管理 → 修改作品) does. It loads the work first,
prints the change (each field as old → new) and asks (`-y` in scripts).

- TEXT replaces the description; the hashtags stay unless `--topic` is
  given.
- `--title` replaces the title (30 characters for videos, 20 for image
  posts).
- `--topic NAME` (repeatable) replaces the hashtags, which go after the
  text as they do on `post`; `--no-topics` removes them. Without TEXT the
  description stays and only its hashtags change.
- `--cover IMAGE` uploads a new cover.
- `--visibility public|friends|private` goes through the manage page's
  permission setting; it does not count as an edit.
- Douyin limits how often a work's text, title and cover may change; the
  summary shows the edits used, and a work that can no longer change
  fails with `permission_denied` and Douyin's reason. Changed works may be
  reviewed again.
- Works whose text @-mentions people or joins an activity are refused
  (ATP cannot write those back), as are articles.

```
atp douyin edit 7697611474302225714 --topic 操作系统 --topic 计算机
atp douyin edit https://www.douyin.com/video/7697611474302225714 --title '『操作系统 Ep.02』硬件视角'
atp douyin edit '#1' --visibility private
```

### delete

Deletes one of your works through the creator platform (the id, a link,
or `#N`), after asking (`-y` in scripts).

### mixes

Your collections (合集) on the creator platform, with their episode
counts; `post --collection` takes a name or an id from here. Creating a
collection is not offered.

### musics

The music library of image posts and articles: without an argument its
categories (`TYPE:ID`), with one the songs of that category, each with
its id and length in seconds. Searching the library is not offered (the
page searches through a separately signed service).

### download

Videos at the highest bitrate without watermark, every image of an image
post (with live-photo clips) and its music; `--audio-only` saves the music.

## Limits

- Publishing and deleting are built from the contract of the creator
  platform's web client (its page bundles), and have not been run against
  a real account yet.
- Formats the web uploader also offers that ATP does not: panoramic
  video (全景视频), which needs the projection settings and a server-side
  transcode change (`vr/transcode/change`) ATP does not reproduce; short
  drama (短剧), which needs a playlet qualification and its own series.
  Daily posts (日常) and text-only posts are made in the app; the web
  uploader has neither.
- Not offered when publishing: mentions (@), locations and other anchors,
  hot topics, co-creators, paid content, choosing a video's music (Douyin
  picks it), editing a cover in the page's editor, and pictures inside
  articles.
- Writes are untested against the live site (see Risk control), and so
  are the following orders, `--search` and `--mutual` (built from the web
  client's page bundles): how `oldest` and `smart` page on, and whether
  the share panel's relations hold all your mutual follows, are not
  confirmed.
- The web offers no follow groups, list of blocked accounts, way to set
  remarks or special follows, or folder covers, descriptions and order,
  so ATP has none of them either.
- No creator analytics yet.
