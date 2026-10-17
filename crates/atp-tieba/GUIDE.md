## Logging in

- `atp tieba login` shows a Baidu passport QR code; scan it with the Tieba
  or Baidu app and confirm. The login saves `BDUSS` and Tieba's `STOKEN`.
- `atp tieba login --browser` imports the cookies of a browser logged in
  at https://tieba.baidu.com (it reads the baidu.com and tieba.baidu.com
  cookies).
- `atp tieba login --cookie 'BDUSS=…; STOKEN=…'` takes the `Cookie` header
  of a tieba.baidu.com request. `BDUSS` is the session; `STOKEN` (Tieba's
  own, set for tieba.baidu.com) is needed by agrees, follows and saved
  threads. `ATP_TIEBA_COOKIE` supplies such a header for one run, without
  saving it.

Most reading works without an account (see the commands below). Logged in
come `collections [USER]` (followed forums with your level), `favorites`
(收藏), `notifications`, `unread`, `user-replies` without a user, and
every write.

How long a session lasts is not known. When Tieba refuses it, commands
fail with `not_authenticated` ("Tieba did not accept the session (BDUSS)")
and a hint to log in again.

Calls imitate the Android app, with a device id ATP makes once and keeps
in the session. Tieba asks new or busy accounts for captchas: a
`verification_required` error means post once in the app or browser and
wait (on a terminal ATP opens tieba.baidu.com in a browser, see `atp guide
login`). Answers such as 太快 or 频繁 become `rate_limited`. Requests are
spaced at least a second apart, and every write waits 0.8 to 2 seconds
first, as bursts get flagged.

## References

- Threads: the id (`10083467821`) or a `https://tieba.baidu.com/p/<id>`
  link. A link with `?pid=<floor>` (or `<thread>/<floor>`) points at one
  floor: `like` then agrees with that floor, `comment` replies inside it.
- Users: the numeric uid, the portrait (`tb.1.…`), a
  `tieba.baidu.com/home/main?id=` / `?un=` link, or a user name (`@name`;
  names resolve through user search and need an exact match).
- Forums (吧): the name (`rust`) or a `/f?kw=` link; `#N` after a forum list
  (`search -t topic`, `collections`).
- Hot topics: `#N` after `hot`, the topic id or its link.

## Commands

Reading without an account: `hot`, `topic`, `forum`, `forum-info`, `read`,
`comments`, `replies`, `search`, `forum-search`, `user`, `user-posts`,
`user-replies USER`, `followers` and `following`.

### hot

The 热议 board.

### topic

`topic TOPIC` lists the threads of a hot topic. `read` refuses a topic
link and points here.

### forum

`forum NAME [--sort reply|new|good]`: by last reply, newest first, or 精品
only.

### forum-info

The forum's members, threads, posts, category and managers.

### read

The first floor of a thread. Floors carry `extra.floor` and `extra.url`;
their pictures are listed in `extra.images`.

### comments

`comments THREAD [--sort asc|desc|author]`: the other floors with their
first replies inline (`--replies` fetches all); `author` keeps the
original poster's floors only (只看楼主).

### replies

`replies THREAD FLOOR` lists the replies inside a floor (楼中楼).

### search

Threads; `--sort relevant|new|old`, `--filter threads|posts`.
`search -t topic` finds forums, `search -t user` users.

### forum-search

`forum-search FORUM QUERY` searches threads inside one forum, with the
same `--sort` and `--filter`.

### user-posts, user-replies

`user-posts` lists the threads a user started, `user-replies` the replies
they wrote.

### collections

`collections [USER]` lists followed forums with your level; it needs a
login, also for another user's.

### favorites

Your saved threads (收藏) only; `favorites USER` is not supported.

### notifications

`--type reply|at|agree`: replies to you, @ mentions, agrees.

### like

`like THREAD` agrees (点赞) with a thread, or with a floor given by link.

### favorite

`favorite THREAD [--undo]` saves the thread.

### comment

`comment THREAD TEXT [-i IMAGE]` adds a floor; `--reply-to FLOOR` (or a
floor link) replies inside that floor. Pictures go up in 512 KB chunks
and are placed after the text.

### post

`post TEXT --topic FORUM [--title T] [-i IMAGE]` starts a thread in that
forum; `--reply-to` and `--quote` are not supported. Pictures go up as for
`comment`.

### delete, delete-comment

`delete THREAD`, `delete-comment THREAD FLOOR`; for a reply inside a floor
name the floor in THREAD (`THREAD/FLOOR`) and the reply as COMMENT.

### follow, follow-forum, unfollow-forum

`follow USER` follows a user; `follow-forum FORUM [--undo]` and
`unfollow-forum FORUM` join and leave forums.

### remove-follower

`remove-follower USER… [-y]` (移除粉丝) asks first, then sends the
client's `removeFans` with each user's numeric id, one call per user.
Blocking (拉黑) goes through the app's protobuf API and is not offered.

### sign

`sign FORUM…` signs in (签到); `sign --all` signs every followed forum.
Each forum gets a row with `ok`, `already` (signed today), the bonus and
the days in a row. A lost session, a captcha or a rate limit stops the
batch.

### download

`download THREAD` saves the first floor's pictures, video and voice.

## Limits

No creator analytics, and the personalized home feed needs the app's
protobuf API. Writes and the QR login follow the Android client and Baidu
passport but were not exercised against a live account.
