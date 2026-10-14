## Logging in

- `atp ncm login` shows the web player's QR code; scan it with the
  NetEase Cloud Music (网易云音乐) app and confirm.
- `atp ncm login --browser` imports the cookies of a browser logged in at
  https://music.163.com, and `--cookie '...'` takes them pasted (the whole
  `Cookie` header of a music.163.com request; `MUSIC_U` is required).
- There is no SMS or password login. When NetEase wants a human check
  (codes -462, 8821 and the like) the command fails with
  `verification_required`, with the page to open when NetEase names one;
  pass it in the app or the browser and run the command again.

Search, songs, lyrics, albums, playlists, artists, podcasts, comments,
charts and profiles work without an account. Daily recommendations, your
history, liked songs of yours, the cloud drive and every change need one.

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

## Music

- `search Q` finds songs; `--filter album|artist|playlist|podcast` other
  kinds, `-t user` accounts.
- `read SONG` gives the song with its lyrics, each translated line under
  its original; `extra` holds the artists, album, duration and who wrote
  the lyrics down. `read` on an album or a playlist lists its tracks (the
  first 100 of a playlist), on an artist their top songs, on a podcast its
  latest episodes.
- `lyrics SONG [--lrc] [--original]`: only the lyrics, as plain text or
  LRC (`--json` and `.data.text` give the bare text for a file).
- `hot [--category C]`: a chart (榜单); `hot` is 热歌榜, `soaring` 飙升榜,
  `new` 新歌榜, `original` 原创榜, and there are genre, language and
  overseas charts. `toplists` lists every chart; read one with
  `playlist tracks playlist:ID`.
- `feed`: today's recommended songs (每日推荐), with the reason when given.
- `comments POST [--sort hot|new|recommended]`, `replies POST COMMENT`.
- `user USER`, `user-posts USER` (the playlists they made), `followers`,
  `following`, `ranking [USER] [--all-time]` (听歌排行, when they show it).

## Library

- `like SONG [--undo]` 红心s a song; `likes [USER]` and `favorites [USER]`
  list liked songs, `favorites --folder playlists` collected playlists and
  `--folder albums` your collected albums.
- `favorite PLAYLIST|ALBUM|ARTIST|PODCAST [--undo]` collects (收藏) it.
- `history`: the songs you played lately, with `extra.played_at`.
- `follow USER [--undo]`, `comment POST TEXT [--reply-to ID]`,
  `delete-comment POST ID`.
- `playlist create NAME [--private]`, `playlist add PLAYLIST SONG…`,
  `playlist remove PLAYLIST SONG…`, `playlist delete PLAYLIST [--yes]`,
  `playlist tracks PLAYLIST` (any playlist, page by page).
- `signin`: the daily check-in (每日签到).

## Cloud drive (云盘)

- `cloud ls`, `cloud quota`.
- `cloud upload FILE… [--artist A] [--album B]`: your own audio files (mp3,
  flac, ape, wav, m4a, ogg …). A file the drive holds already by md5 is
  done at once (`instant: true`); others go up in 4 MB parts, and a stopped
  upload continues with its next part when run again within 6 hours.
- `cloud download FILE… [-o DIR]`: files you uploaded, by the ids `cloud
  ls` shows, under the names you gave them. Ids your drive does not list
  are refused.
- `cloud delete FILE… [--yes]`.

## Limits

atp never fetches the audio of catalog songs or podcast episodes: no
playback or download links, nothing around VIP, trials or encrypted
`.ncm` files. `download POST` saves the cover image only. Requests are
spaced at least a second apart.
