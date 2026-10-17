## Logging in

- `atp weibo login` shows a QR code; scan it with the Weibo app (我的 →
  扫一扫) and confirm there. ATP then trades the confirmation for weibo.com
  cookies. When the scan is confirmed but Weibo sets no session cookie,
  the login fails and suggests `--browser` or `--cookie`.
- `atp weibo login --browser` imports the cookies of a browser logged in at
  https://weibo.com (it reads the weibo.com, weibo.cn and sina.com.cn
  cookies).
- `atp weibo login --cookie '...'` takes the whole `Cookie` header of a
  weibo.com request; it must contain `SUB`. The `XSRF-TOKEN` that writes
  send is fetched when the session lacks it. `ATP_WEIBO_COOKIE` supplies
  such a header for one run, without saving it.

Without a login, ATP uses a visitor cookie from Sina's visitor system
(kept for up to three days, apart from the session, and renewed once when
Weibo refuses it): hot lists, search, posts, comments, likers, profiles, a
user's posts, followers and following all work. Search and `topic` always
go through m.weibo.cn with the visitor cookie, logged in or not. Reposts
lists, the feeds other than `hot`, notifications, unread counters, your
favorites and their tags, follow groups, blocks, special follows and likes,
`following --sort` / `--search`, insights and every write need a
login. When Weibo shows something to logged-in accounts only, the command
fails with `not_authenticated`.

How long a session lasts is not known. When Weibo refuses it, commands
fail with `not_authenticated` ("the Weibo session has expired or was
refused") and a hint to log in again.

Weibo asks for a slider check when an account writes too much; the
command then fails with `verification_required`, and on a terminal ATP
opens weibo.com in a browser to pass it (see `atp guide login`). Answers
such as 频繁 or 操作过快 become `rate_limited`: slow down and retry later.
Requests are spaced at least a second apart, and every write waits 1.2 to
3.5 seconds first.

## References

- Posts: `https://weibo.com/<uid>/<id>`, `https://weibo.com/detail/<mid>`,
  `https://m.weibo.cn/status/<id>`, `https://m.weibo.cn/detail/<mid>`, the
  numeric id (`mid`, 16 digits) or its short form (`bid`, e.g. `RmbnfkgUR`).
- Users: the numeric id, `@screen name` (or the bare name), a personal
  domain, or a `weibo.com/u/<uid>`, `weibo.com/n/<name>` or `m.weibo.cn/u/<uid>` link.
- Comments: the numeric comment id (`comments` lists them).

## Commands

### hot

`--category realtime` (热搜, the default), `posts` (热门微博), and the
boards `entertainment`, `life`, `social`, `tech`, `sports`, `acg`,
`trend`, `explode` (these two come back empty without a login) and `mine`
(yours, logged in). Hot-search entries are posts of kind `trend` with the
heat in `metrics.views`.

### feed

`--type following|latest|original|video|hot`; logged in the default is
`following`, logged out only `hot` works.

### search

`--sort general|latest|hot`, `--filter video|image`; `-t user`,
`-t topic` (with discussion and read counts).

### comments

`--sort hot|time`, `--all --replies` for whole threads.

### comment, delete-comment

`comment POST TEXT --reply-to COMMENT` answers a comment;
`delete-comment POST COMMENT` takes the comment id.

### followers, following

- On your own lists, rows carry `extra.mutual` (they follow you back, or
  you follow back a follower) and, on whom you follow, `extra.special`
  (特别关注) and `extra.remark`, as far as Weibo includes them.
- `following --sort recent` (最新关注, latest follow first) or `--sort
  posted` (最新发博: the accounts that posted last first; Weibo lists only
  those that posted in the last seven days) read the relation page, which
  shows your own following only; `--sort recent` of another user is
  their plain list.
- `following --search WORDS` searches your own following by name (no
  `--sort`). There is no `--mutual`.
- Weibo hides some accounts of long lists (自保护策略), so a list can end
  before its count.

### following-filter

`following-filter --within 7d|30d|180d` lists your followings by when you
followed them; `--inactive` the ones that posted nothing in the last 30
days. Act on the result with `unfollow '#N' ...`.

### follow, unfollow

Unfollowing several users goes through the relation page's batch manager,
20 per call; every user is looked up first, and a name that finds nobody
stops the command before anything changes. Following several goes one by
one.

### remove-follower

`remove-follower USER… [-y]` (移除粉丝) asks first; one call per user, and
they are not told.

### block, unblock, blocked

- `block USER…` blocks completely (拉黑: their posts hidden, no
  interaction, no following you), one call per user; screen names are
  sent as they are.
- `unblock` takes users off the blacklist; `blocked` lists it as the
  settings page shows it, with what is blocked in `extra.blocking`. Those
  rows carry no user id: `id` is the screen name, which `unblock '#N'`
  takes.

### special, special-follows

`special USER… [--undo]` marks special follows (特别关注), whose posts
Weibo puts first in the feed; `special-follows` lists yours.

### remark

`remark USER TEXT` sets the name you see for someone you follow (备注);
`remark USER ''` clears it.

### follow-groups, follow-group, follow-group-create, follow-group-rename, follow-group-delete, follow-group-add, follow-group-remove

- `follow-groups` lists your groups with `items` (members) and
  `extra.mode`: `public`, `private` or `system` (groups Weibo keeps,
  which cannot be renamed or deleted). GROUP is an id, `#N` or a group's
  exact name.
- `follow-group GROUP` lists its people; there is no `--sort`.
- `follow-group-create NAME` makes a private group; `follow-group-rename`
  keeps whether it is public.
- `follow-group-add` / `follow-group-remove GROUP USER…`: Weibo sets a
  user's groups all at once, so ATP reads each user's groups first and
  sends them back with GROUP added or taken out, one user at a time.

### group

`group GROUP` reads the latest posts of one of your follow groups (ids
from `follow-groups`).

### favorite

`favorite POST --folder TAG` favorites the post and puts it under a tag
(as `folder-add`). `unfavorite` takes no `--folder`: it drops the
favorite with all its tags; `folder-remove` takes it off one tag.

### favorites, collections

Weibo shows favorites and their tags of the logged-in account only;
another user's fail with `permission_denied`. `collections` lists your
tags (收藏标签) as folders; `favorites --folder TAG` lists what is under
one, and each favorite carries its tags' names in `extra.folders`.

### folder-edit, folder-delete, folder-add, folder-remove, folder-move

Favorite tags are Weibo's folders. A favorite is under at most two tags,
and a tag exists while favorites carry it, so there is no
`folder-create`: `folder-add` and `folder-move` take a new name and make
the tag. Names are up to 12 Chinese characters (24 letters) of Chinese,
letters, digits, `_` and `:`.

- FOLDER is a tag id, `#N` after `collections`, or its exact name.
- `folder-edit TAG --name NEW` renames a tag; Weibo may give it a new id,
  which the result carries. Tags have no description, visibility or
  cover.
- `folder-delete TAG… [-y]` deletes tags; their favorites stay.
- Weibo shows a favorite's tags only in the favorites list, so the
  folder commands first read your favorites (`folder-remove` and
  `folder-move` only the source tag's) until every post turned up, up to
  50 pages; a favorite older than those fails. `folder-add` first reads
  each post: one that is no favorite yet is favorited, then tagged,
  without that search.
- `folder-add TAG POST…` keeps a favorite's other tag and fails for one
  already under two; `folder-remove TAG POST…` keeps it a favorite.
- `folder-move FROM TO POST… [--copy]` changes each favorite's tags in
  one call.

### topic

`topic NAME` lists posts under a hashtag, newest first.

### post

`post TEXT [--title T] [-i IMAGE ...] [--topic T]`: up to 18 pictures;
topics become `#T#`, and a title goes on the first line. `post TEXT
--quote POST` reposts with text (no pictures then); `--reply-to` is not
supported, use `comment`.

### repost

`repost POST [TEXT] [--comment]` reposts (转发), optionally with your text;
`--comment` also posts the text as a comment on the original.

### edit

Changes one of your posts the way weibo.com's post menu does. It loads the
post first, prints the change (each field as old → new) and asks (`-y` in
scripts).

- TEXT replaces the text; topics and mentions are written in it
  (`#topic#`, `@name`). The pictures stay as they are. Editing the text
  is for Weibo members (VIP and up), who get a number of edits a day; the
  summary shows the edits left, and without any the edit fails with
  `permission_denied`.
- Weibo edits posts with text and pictures only, published after
  2017-10-01, not promoted or part of an activity; it refuses others with
  its reason. Posts with a video, mixed media or live photos, and posts
  shown to a group or to paying fans, are refused before asking.
- `--visibility public|friends|private` changes who can see the post
  (friends is the friends circle, 好友圈); it needs no membership. A
  pinned post must be unpinned first.
- Edits and visibility changes share a limit per post (100 in all), and
  everyone can see a post's edit history.

```
atp weibo edit https://weibo.com/1234567890/Pxyz12345 '改过的文字 #话题#'
atp weibo edit 5012345678901234 --visibility private
```

### delete

Deletes one of your own posts.

### notifications

`--type comments` (received, the default), `mentions` (in posts),
`comment-mentions`, `sent-comments` and `likes`.

### insights

`insights` reports what weibo.com shows: lifetime followers, posts and the
likes, comments and reposts received, plus yesterday's reads and
interactions. `insights POST` gives a post's counters (reads where Weibo
shows them, video plays). The creator data center (me.weibo.com) has no
usable web API, so there are no daily trends or audience breakdowns.

### download

Pictures at their largest size (live photos with their motion clip, GIFs
as video) and videos in the best quality offered.
