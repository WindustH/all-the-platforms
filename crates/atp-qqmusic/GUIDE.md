## Logging in

- **QR code**: `atp qqmusic login` shows QQ's login QR code; scan it
  with the QQ app (手机 QQ, not the QQ Music app) and confirm. ATP then
  turns the QQ login into a QQ Music session the way y.qq.com does (QQ
  Connect) and keeps only the QQ Music cookies (`uin`, `qqmusic_key`,
  `qm_keyst` …); the QQ account's own cookies (`skey`, `p_skey` …) are
  dropped.
- **Browser**: log in at https://y.qq.com (with QQ or WeChat), then run
  `atp qqmusic login --browser`; it reads the cookies of qq.com and keeps
  the QQ Music ones.
- **Cookie string**: copy the whole `Cookie` header of a y.qq.com request
  and run `atp qqmusic login --cookie '...'`; it needs `qqmusic_key` (or
  `qm_keyst`) and `uin` (`wxuin` and `wxopenid` for WeChat accounts).
  `ATP_QQMUSIC_COOKIE` supplies such a header for one run and is never
  saved.

There is no WeChat QR, SMS or password login: WeChat accounts log in at
y.qq.com and use `--browser` or `--cookie`.

Search, songs with their lyrics, albums, playlists, singers, charts,
comments and replies work without an account, and so do the playlists a
user made (`user-posts`) and the liked songs and collected playlists and
albums of a USER (`likes USER`, `favorites USER`). QQ Music shows
profiles to logged-in visitors only: without a login `user` gives just
the nickname and the number of playlists. Picks for you (`feed`),
`followers`, `following`, your own likes and collections, and every
change need a login.

How long a session lasts is not known, and ATP does not renew it. When
QQ Music stops accepting it (codes 1000, 104400, 104401), commands fail
with `not_authenticated` and the hint to log in again; `whoami` says "QQ
Music does not accept this session any more".

### Risk control

- When QQ login wants more than a scan (a new device, an unusual place),
  `login` fails with `verification_required`, with the page to open when
  QQ names one; pass it, or log in at y.qq.com and use `--browser`.
- Code 2001 means QQ Music wants a human check or a login
  (`verification_required`, with its page when it sends one). Without a
  login it now and then answers a search this way; wait a little and
  retry, or log in.
- 104604 is `rate_limited`; 20277, 20278 and 20450 mean the account is
  restricted and 20279 that too many devices are logged in
  (`permission_denied`).
- Every call is signed as the web client signs it; a refused signature
  (2000) is reported as such.
- Requests are spaced at least a second apart, and every change (likes,
  collections, follows, comments, playlists) waits another 0.5 to 1.5 s.
- QQ Music profiles keep a list of visitors (访客); whether `user` while
  logged in puts you on it is not known.

## References

- Songs: the mid (`0039MnYb0qxYhV`), the number (`97773`), `song:MID`, or
  a link such as `https://y.qq.com/n/ryqq_v2/songDetail/0039MnYb0qxYhV`
  (older `/n/ryqq/` and `/n/yqq/song/MID.html` links,
  `i.y.qq.com/v8/playsong.html?songmid=` pages and `c6.y.qq.com` short
  links work too). A bare mid or number is a song.
- Albums `album:MID` (or its number), playlists `playlist:ID`, singers
  `singer:MID` (`artist:MID` too), charts `toplist:ID`, or their y.qq.com
  links.
- In `playlist add|remove|delete|tracks`, a bare number is the playlist.
- Users: the encrypted id QQ Music shows (`oK6FoK6q7e657n**`, as
  `search -t user`, comments and playlists give it), an account number, a
  `profile?uin=` link, or `@NAME` for an exact nickname.
- Comments: the ids `comments` lists (`1!Yn6w…*`; quote them in the
  shell); `comment POST TEXT --reply-to ID` answers one, `replies POST
  ID` lists the replies under it.

## Commands

### search

`search Q` finds songs; `--filter album|artist|playlist` other kinds,
`--filter lyrics` songs whose lyrics match (the matching line in
`extra.lyrics_match`), `-t user` accounts. It works without a login.

### read

`read SONG` gives the song with its lyrics, each translated line under
its original; `extra` holds the singers, album, duration, number
(`song_id`) and what the song page says (introduction, genre, language,
company). `read` on an album lists its tracks, on a playlist its first
100 songs and its number of comments, on a singer their top 20 songs and
fans, on a chart its first 100 songs.

### download

`download POST` saves the cover image only (see Limits).

### lyrics

`lyrics SONG [--lrc] [--original]`: only the lyrics, as plain text or LRC
(`--json` and `.data.text` give the bare text for a file). QQ Music sends
lyrics base64 encoded; ATP decodes them.

### hot

`hot [--category C]`: a chart (榜单); `hot` is 热歌榜, `soaring` 飙升榜,
`new` 新歌榜, `popular` 流行指数榜, and there are region, genre and
overseas charts. Each song has its place in `extra.rank`.

### toplists

Lists every chart with its group (`extra.group`); read one with `read
toplist:ID`, list its songs with `playlist tracks toplist:ID`.

### feed

Songs QQ Music picks for you (猜你喜欢, the personal radio): one batch
per run, a new one each time. Needs a login. 每日30首 is not offered: no
plain web call serves it.

### comments, replies

`comments POST [--sort hot|new|recommended]` on songs, albums and
playlists; `replies POST COMMENT` lists the replies under a comment,
oldest first. A comment's first replies also come inline (`replies`).

### user, user-posts, followers, following

`user USER` (see Logging in for what shows without a login), `user-posts
USER` (the playlists they made; system ones without an id, such as
QZone背景音乐, are left out). `followers USER` and `following USER` (the
users they follow) need a login.

### likes, favorites

`likes [USER]` and `favorites [USER]` list liked songs (我喜欢),
`favorites --folder playlists` collected playlists and `--folder albums`
collected albums. A USER's work without a login; yours need one.

### like

`like SONG [--undo]` likes a song (喜欢: adds it to 我喜欢); songs only.

### favorite

`favorite PLAYLIST|ALBUM [--undo]` collects (收藏) it. Songs are liked
instead, or added with `playlist add`; singers are followed.

### follow

`follow USER [--undo]` follows (关注) a user; `follow singer:MID` (or a
singer link) a singer.

### comment, delete-comment

`comment POST TEXT [--reply-to ID]`, `delete-comment POST ID`.

### playlist

Your playlists, and any playlist's or chart's songs.

### playlist create

`playlist create NAME`: at most 20 Chinese characters or 40 letters. New
playlists are public; QQ Music's privacy setting is not offered.

### playlist add, playlist remove

`playlist add PLAYLIST SONG…`, `playlist remove PLAYLIST SONG…`, on
playlists of yours (by the ids `user-posts` shows).

### playlist delete

`playlist delete PLAYLIST [--yes]`; 我喜欢 cannot be deleted.

### playlist tracks

`playlist tracks PLAYLIST`: any playlist or chart (`toplist:ID`), page by
page.

## Limits

- ATP never fetches the audio of catalog songs: no playback or download
  links (vkey, purl), nothing around VIP, trials or encrypted formats.
  `download POST` saves the cover image only.
- There is no `history` (recently played): QQ Music serves it through no
  plain web call.
- No WeChat QR or SMS login, no captcha solving.
- ATP calls the web gateway as QQ Music's own client does (its user
  agent), and signs every call the way the web client signs it; QQ login
  goes as a browser.
