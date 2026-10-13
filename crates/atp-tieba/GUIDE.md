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

## Reading

Most reading works without an account: `hot` (the 热议 board), `topic
TOPIC` (its threads), `forum NAME [--sort reply|new|good]`, `forum-info
NAME`, `read THREAD` (the first floor), `comments THREAD [--sort
asc|desc|author]` (the other floors with their first replies inline;
`--replies` fetches all), `replies THREAD FLOOR` (楼中楼), `search` (threads;
`--sort relevant|new|old`, `--filter threads|posts`), `search -t topic`
(forums), `search -t user`, `forum-search FORUM QUERY`, `user`,
`user-posts` (threads), `user-replies`, `followers` and `following`.

Floors carry `extra.floor` and `extra.url`; their pictures are listed in
`extra.images`. `download THREAD` saves the first floor's pictures, video
and voice.

## Logging in

`atp tieba login` shows a Baidu passport QR code for the Tieba or Baidu
app; `--browser` (a browser logged in to tieba.baidu.com) and `--cookie
'BDUSS=…; STOKEN=…'` work too. `BDUSS` is the session; `STOKEN` (Tieba's
own, set for tieba.baidu.com) is needed by agrees, follows and saved
threads.

Logged in: `collections [USER]` (followed forums with your level),
`favorites` (收藏), `notifications --type reply|at|agree`, `unread`.

## Writing

- `like THREAD` agrees (点赞) with a thread, or with a floor given by link.
- `favorite THREAD [--undo]` saves the thread.
- `comment THREAD TEXT [-i IMAGE]` adds a floor; `--reply-to FLOOR` (or a
  floor link) replies inside that floor.
- `post TEXT --topic FORUM [--title T] [-i IMAGE]` starts a thread.
- `delete THREAD`, `delete-comment THREAD FLOOR`; for a reply inside a floor
  name the floor in THREAD (`THREAD/FLOOR`) and the reply as COMMENT.
- `follow USER`, `follow-forum FORUM [--undo]`, `unfollow-forum FORUM`.
- `sign FORUM…` signs in (签到); `sign --all` signs every followed forum.

Pictures go up in 512 KB chunks and are placed after the text. Tieba asks
new or busy accounts for captchas: a `verification_required` error means
post once in the app or browser and wait.

## Limits

No creator analytics, and the personalized home feed needs the app's
protobuf API. Writes and the QR login follow the Android client and Baidu
passport but were not exercised against a live account.
