## Logging in

- **QR code**: `atp ncm login` shows the web player's QR code; scan it
  with the NetEase Cloud Music (网易云音乐) app and confirm. The login is
  done once NetEase sends the cookie `MUSIC_U`.
- **Browser**: log in at https://music.163.com, then run `atp ncm login
  --browser`; it reads the cookies of music.163.com.
- **Cookie string**: copy the whole `Cookie` header of a music.163.com
  request and run `atp ncm login --cookie '...'`; `MUSIC_U` is required.
  `ATP_NCM_COOKIE` supplies such a header for one run and is never
  saved.

There is no SMS or password login.

Search, songs, lyrics, albums, playlists, artists, podcasts, comments,
charts and profiles work without an account. Daily recommendations
(`feed`), your history, your own liked songs and collections, the
listening ranking without a USER, the cloud drive, `signin` and every
change need one.

How long a session lasts is not known. When NetEase stops accepting it
(codes 301, 50000005), commands fail with `not_authenticated` and the
hint to log in again; `whoami` says "NetEase does not accept this
session any more".

### Risk control

- When NetEase wants a human check (codes -462, -463, 8821) the command
  fails with `verification_required`, with the page to open when NetEase
  names one; pass it in the app or the browser and run the command
  again.
- `-460` means NetEase refuses requests from this network for now
  (`ip_blocked`): retry later, or log in.
- `405`, `406` and `-447` are `rate_limited`.
- Requests are spaced at least a second apart, and every change (likes,
  follows, comments, playlists, uploads) waits another 0.5 to 1.5 s.

## References

- Songs: the id (`186016`), `song:ID`, or a link such as
  `https://music.163.com/song?id=186016` (`/#/song?id=`, `/m/song?id=` and
  `163cn.tv` share links work too). A bare number is a song.
- Albums `album:ID`, playlists and charts `playlist:ID`, artists
  `artist:ID`, podcast shows `podcast:ID` (`djradio?id=` links) and
  episodes `episode:ID` (`program?id=` links), or their links.
- In `playlist add|remove|delete|tracks`, a bare number is the playlist.
- Users: the numeric id, a `user/home?id=` link, or an exact nickname.
- Comments: their numeric ids from `comments`; `comment POST --reply-to ID`
  answers one, `replies POST ID` lists its floor (楼层).
- Cloud drive files: the ids `cloud ls` shows, or `#N` after it.

## Commands

### search

`search Q` finds songs; `--filter album|artist|playlist|podcast` other
kinds, `-t user` accounts. It works without a login.

### read

`read SONG` gives the song with its lyrics, each translated line under
its original; `extra` holds the artists, album, duration and who wrote
the lyrics down. `read` on an album or a playlist lists its tracks (the
first 100 of a playlist), on an artist their top songs, on a podcast its
latest episodes.

### download

`download POST` saves the cover image only (see Limits).

### lyrics

`lyrics SONG [--lrc] [--original]`: only the lyrics, as plain text or
LRC (`--json` and `.data.text` give the bare text for a file).

### hot

`hot [--category C]`: a chart (榜单); `hot` is 热歌榜, `soaring` 飙升榜,
`new` 新歌榜, `original` 原创榜, and there are genre, language and
overseas charts.

### toplists

Lists every chart; read one with `playlist tracks playlist:ID`.

### feed

Today's recommended songs (每日推荐), with the reason when given. Needs
a login.

### comments, replies

`comments POST [--sort hot|new|recommended]` on songs, albums,
playlists and episodes; `replies POST COMMENT` lists a comment's floor.

### user, user-posts, followers, following

`user USER`, `user-posts USER` (the playlists they made), `followers`,
`following`.

### ranking

`ranking [USER] [--all-time]`: the listening ranking (听歌排行) of the
last week or of all time, when the user shows it; yours without USER.

### like

`like SONG [--undo]` 红心s a song; songs only (playlists, albums,
artists and podcasts take `favorite`).

### likes, favorites

`likes [USER]` and `favorites [USER]` list liked songs, `favorites
--folder playlists` collected playlists and `--folder albums` your
collected albums (not those of others). Yours need a login.

### favorite

`favorite PLAYLIST|ALBUM|ARTIST|PODCAST [--undo]` collects (收藏) it.
Songs are liked instead, or added with `playlist add`; episodes cannot be
collected.

### history

The songs you played lately, with `extra.played_at`.

### follow, comment, delete-comment

`follow USER [--undo]`, `comment POST TEXT [--reply-to ID]`,
`delete-comment POST ID`.

### playlist

Your playlists, and any playlist's songs.

### playlist create

`playlist create NAME [--private]`.

### playlist add, playlist remove

`playlist add PLAYLIST SONG…`, `playlist remove PLAYLIST SONG…`, on
playlists of yours.

### playlist delete

`playlist delete PLAYLIST [--yes]`.

### playlist tracks

`playlist tracks PLAYLIST`: any playlist or chart, page by page.

### signin

The daily check-in (每日签到).

### cloud

Your music cloud drive (云盘): only files you uploaded yourself. Needs
a login.

### cloud ls, cloud quota

`cloud ls` lists your files, newest first; `cloud quota` the space used
and allowed.

### cloud upload

`cloud upload FILE… [--artist A] [--album B]`: your own audio files (mp3,
flac, ape, wav, m4a, ogg …). A file the drive holds already by md5 is
done at once (`instant: true`); others go up in 4 MB parts, and a stopped
upload continues with its next part when run again within 6 hours.

### cloud download

`cloud download FILE… [-o DIR]`: files you uploaded, by the ids `cloud
ls` shows, under the names you gave them. Ids your drive does not list
are refused.

### cloud delete

`cloud delete FILE… [--yes]`.

## Limits

- ATP never fetches the audio of catalog songs or podcast episodes: no
  playback or download links, nothing around VIP, trials or encrypted
  `.ncm` files. `download POST` saves the cover image only.
- ATP talks to NetEase as the web player and, for search, cloud uploads
  and some comment pages, as the desktop client; both kinds of call are
  encrypted the way those clients do it.
