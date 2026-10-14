ATP talks to Spotify only through the official Web API, with an app you
register yourself. It never uses the web player's internal tokens and
never touches audio.

## Logging in

There is one way in: OAuth with your own Spotify app (Authorization Code
with PKCE, so no client secret is needed or stored).

1. Open https://developer.spotify.com/dashboard, log in and create an app.
   Since February 2026 the app owner needs an active Spotify **Premium**
   subscription (the app stops working while it lapses), and an app in
   development mode allows at most **five users**.
2. Under **Redirect URIs** add exactly `http://127.0.0.1:8888/callback`
   and tick **Web API**. Spotify accepts only a loopback IP literal here:
   `localhost` is refused, and `http` is allowed for loopback addresses
   only. To use another port, set `SPOTIFY_REDIRECT_PORT` and register
   `http://127.0.0.1:PORT/callback` instead (or register
   `http://127.0.0.1/callback` without a port, which Spotify matches for
   any port on a loopback address).
3. Under **User Management**, add the Spotify accounts that will log in
   (name and e-mail). Others are refused with "User not registered in the
   Developer Dashboard" (`permission_denied`).
4. Copy the app's **Client ID** (32 hexadecimal digits) and run
   `atp spotify login --token CLIENT_ID`, or set `SPOTIFY_CLIENT_ID` and
   run `atp spotify login --token env`.
5. ATP opens the consent page in your default browser and prints its
   link too. Log in there with your Spotify account and agree; the
   browser comes back to `127.0.0.1`, where ATP listens once (for up to
   five minutes) and takes the code. When Spotify shows "INVALID_CLIENT:
   Invalid redirect URI" instead, the URI of step 2 is missing: add it,
   stop the waiting login with Ctrl-C and run it again.

The login asks only for the permissions of the commands ATP has:
`user-read-private` (your profile), `user-library-read` and
`user-library-modify` (Liked Songs, saved albums, shows and episodes),
`playlist-read-private`, `playlist-modify-public` and
`playlist-modify-private` (your playlists), `user-follow-read` and
`user-follow-modify` (follows), `user-top-read` (`insights`),
`user-read-recently-played` (`history`), and
`user-read-playback-state`, `user-modify-playback-state` and
`user-read-currently-playing` (`player`).

The access token, the refresh token, their expiry and the client id are
kept in ATP's session file for Spotify (readable by you only). Nothing
works without logging in: every Web API call needs a user's token.

An access token lasts about an hour; ATP renews it with the refresh token
ten minutes before it runs out and saves the new pair at once (Spotify
may hand out a new refresh token). How long a refresh token lasts is not
documented by Spotify. When Spotify refuses it (you removed the app's
access, or the app was deleted), commands fail with `not_authenticated`
("the token could not be renewed") and the hint to log in again.
`atp spotify logout` forgets the tokens; the access itself is revoked at
https://www.spotify.com/account/apps/.

Environment: `SPOTIFY_CLIENT_ID` (the app for `login --token env`, and for
renewing tokens when the session lacks its client id),
`SPOTIFY_REDIRECT_PORT` (the port of the redirect URI, 8888 by default).

### Risk control

- Spotify limits requests per app over a rolling 30-second window; since
  July 2026 the quotas of development-mode apps count per developer
  account, shared by all its apps. Too many requests answer HTTP 429
  (`rate_limited`) with the wait Spotify asks for; reads are retried twice
  after a pause. ATP spaces requests at least 0.3 s apart.
- A missing permission (after a scope change) is `permission_denied` with
  the hint to log in again; playback control without Premium is
  `permission_denied` too.
- This is Spotify's own API used as documented; no risk of a ban for
  ordinary use is known. Spotify's developer terms apply to what you do
  with the data (they forbid, for instance, training AI models on it).

## References

- Tracks, albums, artists, playlists, shows and episodes: open.spotify.com
  links (also with `intl-xx/`, `embed/` and `?si=…`), Spotify URIs
  (`spotify:track:ID`, "Copy Spotify URI" in the app), `kind:ID`
  (`track:`, `album:`, `artist:`, `playlist:`, `show:`, `episode:`; also
  `song:` and `podcast:`), spotify.link short links, or a bare
  22-character id. A bare id is a track, except in `follow` (an artist)
  and `playlist …` (a playlist).
- Users: a profile link, `spotify:user:ID`, `user:ID`, an account id or
  `me`. Spotify serves development-mode apps only the logged-in account, so
  every USER argument must name you (see Limits).
- `follow` takes artists, users and playlists.
- Devices (`player … --device`, `player transfer`): `#N` from `player
  devices`, the device id or its name.
- `#N` refers to the last list, as everywhere.

## Commands

### search

`search Q [--filter track|album|artist|playlist|show|episode]` (tracks by
default). Spotify's field filters work in Q: `artist:`, `album:`,
`track:`, `year:1990-1999`, `genre:`, `isrc:`, `upc:`, `tag:new`. Spotify
answers at most ten results a page; `-n` follows the pages, up to the
thousandth result. Spotify has no user search.

### read

- A track: artists, album, duration, ISRC, release date, explicit.
- An album: every track (up to 500), numbered in the text and in
  `extra.tracks`.
- An artist: name, picture, genres when Spotify still sends them, and
  their latest ten albums and singles with the total (`extra.albums`,
  `extra.album_count`). Top tracks are not offered (see Limits).
- A playlist: its details, and its first items when it is yours or you
  collaborate on it; Spotify shows other playlists without their items.
- A show: its description and latest episodes. An episode: its
  description, duration, show and where you stopped (`extra.resume_point`).

### download

`download POST` saves the cover image (the largest Spotify has, usually
640 px): the album's for a track, the picture of an artist, a playlist, a
show or an episode. No audio.

### user, user-posts

`user me` (or your own id or link) shows your profile, as `whoami` does.
`user-posts` lists the playlists you made, public and private
(`extra.public`, `extra.collaborative`, `extra.track_count`). Other users
cannot be looked up.

### following

The artists you follow (Spotify lists followed artists only, not users).
`#N` then works with `unfollow`.

### follow, unfollow

`follow ARTIST|USER|PLAYLIST [--undo]`: follows an artist or a user, or
follows (saves) a playlist into your library. It goes through Spotify's
library endpoint (`/me/library`), which replaced the follow endpoints in
February 2026; its reference lists user and playlist URIs but no artist
URIs, so whether Spotify follows artists through it for every app is not
known.

### favorites, likes

`favorites [--folder tracks|albums|shows|playlists]`: your Liked Songs
(the default), saved albums, saved shows, or the playlists of others you
follow (yours are `user-posts`), with when you saved them
(`extra.added_at`). `likes` is Liked Songs too.

### like, unlike

`like TRACK [--undo]` adds a track to Liked Songs; tracks only.

### favorite, unfavorite

`favorite ALBUM|SHOW|EPISODE [--undo]` saves an album, a show or an
episode (Your Episodes) to your library. Tracks are liked; artists, users
and playlists are followed.

### history

Tracks you played lately, latest first, with when (`extra.played_at`) and
from what (`extra.context`, an album, playlist or artist URI). Spotify
leaves podcast episodes out, and how far back it goes is not documented.

### insights

Your top tracks and top artists (up to 50 each) as two rankings
(`top_tracks`, `top_artists`; the value is the place). Spotify ranks over
three windows, and `--days` is rounded to the nearest: under 90 days
`short_term` (about the last 4 weeks; the default `--days 30`), under 270
`medium_term` (about 6 months), else `long_term` (about a year).
`extra.time_range` says which. There are no numbers for single items.

### playlist

Your playlists: create them, add and remove items, delete them, and list
the items of one.

### playlist create

`playlist create NAME [-d DESCRIPTION] [--private] [--collaborative]`:
public unless `--private`; `--collaborative` lets people you invite add
items and makes it private, as Spotify requires. The result has its id
and link.

### playlist add, playlist remove

`playlist add PLAYLIST ITEM…` appends tracks or episodes, `playlist
remove PLAYLIST ITEM…` takes out every occurrence of them; up to 100 a
request, more in batches. On playlists of yours or ones you collaborate
on. The result's `id` is the playlist's new snapshot id.

### playlist delete

Spotify has no deleting: `playlist delete PLAYLIST [--yes]` unfollows your
playlist, which takes it out of your library. The playlist itself stays
on Spotify: people who follow it keep it, and you can follow it again.

### playlist tracks

`playlist tracks PLAYLIST`: the items, page by page, with
`extra.added_at` and `extra.added_by`. Spotify lists them for playlists
you own or collaborate on only (others: `permission_denied`).

### player

Playback on your devices through Spotify Connect. It needs Premium and a
device with a Spotify app open (desktop, phone, web player, speaker).
Commands go to the active device, or to the one named by `--device`
(`#N` from `player devices`, an id or a name). With no device active,
commands fail with `not_found` and a hint; without Premium with
`permission_denied`.

### player status

What is playing (`item`), `progress`, the `device` with its volume,
`shuffle`, `repeat` and the `context` it plays from;
`is_playing: false` alone when nothing plays.

### player devices

The devices Spotify Connect sees, with `is_active` and `volume_percent`;
`#N` then picks one for `--device` and `transfer`.

### player play

`player play` resumes; `player play ALBUM|PLAYLIST|ARTIST|SHOW` plays it
from the start; `player play TRACK…` plays tracks and episodes in that
order.

### player pause, player next, player previous, player seek, player volume, player shuffle, player repeat

`seek POSITION` takes seconds, `M:SS` or `H:MM:SS`; `volume 0-100`;
`shuffle on|off`; `repeat track|context|off`.

### player queue

`player queue` lists the current item (`extra.now_playing`) and what
comes next; `player queue ITEM` adds a track or an episode.

### player transfer

`player transfer DEVICE [--play]` moves playback to another device,
playing or as it was.

## Limits

- Spotify restricted development-mode apps in February 2026 (new apps
  from 11 February, existing ones from 9 March): the owner needs Premium,
  an app allows five users, and these are gone, so ATP does not offer
  them:
  - other users' profiles and playlists: `user`, `user-posts`,
    `following`, `favorites` and `likes` work for your own account only;
  - an artist's top tracks, new releases and browse categories: there is
    no `hot`;
  - the items of playlists that are not yours (their details remain);
  - search pages of more than ten results;
  - some fields: popularity, artists' followers, albums' labels,
    available markets, and your country, e-mail and plan.
- Since November 2024 new apps get no recommendations, related artists,
  audio features or analysis, featured or category playlists, previews,
  nor Spotify's own editorial and algorithmic playlists (Discover Weekly,
  Daily Mix …; `read` on them is `not_found`): there is no `feed`.
- No comments (Spotify has none in the API), no lyrics, no audio: ATP
  never plays, records, downloads or decrypts audio, and `download` saves
  cover images only.
- No QR code, browser or cookie login, and no use of open.spotify.com's
  own tokens: only your app's OAuth.
