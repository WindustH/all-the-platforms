## Logging in

YouTube logs in with the cookies of a Google session in a browser.
`atp yt login` (the same as `--browser`) reads them from a browser logged
in at youtube.com. With `--cookie`, copy the whole `Cookie` header of a
request to `www.youtube.com` (developer tools, Network tab): requests
carry the session cookies and are signed with `SAPISID` the way the web
app signs them (`SAPISIDHASH`). Some cookie exports lack `SAPISID`;
`__Secure-3PAPISID` holds the same secret and stands in for it. `login`
checks the session with the account menu before saving it.

Without an account you can search, read videos, Shorts and community
posts, comments, channels and their tabs, playlists, transcripts, live
chats and YouTube Music, and download. The home and subscription feeds,
subscriptions, Watch later, liked videos, your playlists, history,
notifications, `accounts` and every write (likes, comments, subscriptions,
the bell, playlists, live chat messages) need a login.

Requests go out as the first Google account of the session (`authuser`
0). `accounts` lists the channels it can switch to (its own and brand
accounts) and marks the active one `selected`; `user me` is the active
channel.

How long a session lasts is not known. When YouTube stops accepting it,
commands fail with `not_authenticated` ("YouTube did not accept the
session"); log in again.

From some networks YouTube asks to "confirm you're not a bot"; the command
then fails with `verification_required`. Log in (`--browser`), wait a
while, or use another network with `--proxy`.

### Analytics and edits (OAuth)

The cookie session gives `insights` public numbers only, and cannot
change videos. Creator analytics come from the official YouTube Analytics
API and `edit` goes through the YouTube Data API, set up once:

1. In Google Cloud Console create a project and enable the **YouTube
   Analytics API** and the **YouTube Data API v3**.
2. Create an OAuth client of type **Desktop app**.
3. `export YOUTUBE_CLIENT_ID=… YOUTUBE_CLIENT_SECRET=…` and run
   `atp yt oauth`. Open the URL it prints in a browser logged in to the
   channel's Google account and allow access (`yt-analytics.readonly`
   and `youtube.readonly` to read, `youtube.force-ssl` to edit the
   channel's videos). The browser returns to a local port on
   `127.0.0.1`; `oauth` waits 5 minutes for it.
4. Export the `YOUTUBE_REFRESH_TOKEN` it prints and keep it secret.

ATP trades the refresh token for access tokens and caches each until
shortly before it expires. How long Google keeps the refresh token valid
is not known; when Google refuses it, `insights` and `edit` fail with
`not_authenticated` ("Google refused the OAuth credentials"): run
`atp yt oauth` again. If Google sends no refresh token, revoke the app's
access in the Google account and run `oauth` again. A grant made before
`oauth` asked for `youtube.force-ssl` can only read: `edit` stops with
`not_authenticated` and the hint to run `oauth` again and export the new
token. While the Google Cloud project's consent screen is in testing,
Google ends refresh tokens after seven days.

### Environment

- `YOUTUBE_CLIENT_ID`, `YOUTUBE_CLIENT_SECRET`: the OAuth client for
  analytics and edits; `oauth` needs them too.
- `YOUTUBE_REFRESH_TOKEN`: the grant `oauth` prints; with all three set,
  `insights` uses the Analytics API and `edit` works.
- `ATP_YOUTUBE_COOKIE`: a cookie header for one run, never saved.

## References

- Videos: the 11-character id, or `watch?v=`, `youtu.be/`, `/shorts/`,
  `/live/` and `/embed/` links.
- Community posts: the `Ug…` id, a `/post/` link or a `community?lb=` link.
- Channels: `@handle`, the `UC…` id, or `/channel/`, `/c/`, `/user/`, `/@`
  links. `me` (also `self`, `mine`, `@me`) is the logged-in account's
  channel.
- Playlists: the id (`PL…`, `UU…`, `WL` Watch later, `LL` Liked videos …)
  or a link with `list=`.
- Comments: the `Ug…` id or a link with `lc=`; a reply is `THREAD.REPLY`.
- YouTube Music albums: `MPREb…`.

## Commands

### search

`--sort` is `relevance` or `popularity` (YouTube dropped the date and
rating orders). `--filter` takes one value: a type (`video`, `shorts`,
`live`, `movie`), an upload date (`today` … `year`), a length
(`under-3min`, `3-20min`, `over-20min`) or a feature (`hd`, `4k`,
`subtitles`, `creative-commons` …). `-t user` finds channels, `-t topic`
playlists.

### hot

The Hype leaderboard (Trending is gone) or an Explore page
(`--category music|gaming|news|sports|live|learning|podcasts`); it follows
your IP's region.

### feed

`-t home` (the default) or `-t subscriptions`; both need a login.

### read

A video or a community post. Videos add duration, category, hashtags,
chapters, the audience-retention `heatmap` (equal segments, intensity 0–1)
and the `most_replayed` ranges when YouTube has them.

### comments, replies

`--sort top|new`. `comments` works on community posts too.

### user

`user me` is the logged-in account's channel.

### user-posts

The channel's Videos tab; `shorts`, `streams` and `community` read the
other tabs.

### following

Your subscriptions only (`me`); YouTube shows no one else's.

### collections

The playlists of a channel; without a channel (or `me`) your own, with
Watch later and Liked videos.

### favorites, likes

`favorites` is your Watch later, or any playlist with `--folder PLAYLIST`
(no login needed for a public one). `likes` is your Liked videos. Both show
the logged-in account's lists only.

### like, unlike, dislike

Videos only. `dislike VIDEO [--undo]` marks a video disliked; only you see
dislikes.

### favorite, unfavorite

`favorite` adds a video to Watch later, or to one of your playlists with
`--folder PLAYLIST`; `unfavorite` takes it out again.

### comment, delete-comment

Comments go on videos. `--reply-to COMMENT` replies in that comment's
thread. `delete-comment` works on your comments and on comments under your
videos.

### like-comment

`like-comment VIDEO COMMENT` likes a comment; `--dislike` dislikes it
(only you see that), `--undo` takes either back.

### follow, unfollow, bell

`follow` subscribes, `unfollow` unsubscribes.
`bell CHANNEL all|personalized|none` sets the notifications of a
subscription.

### notifications, unread

The bell's inbox and its unseen count.

### transcript, captions

`transcript VIDEO [--lang L]` prints the captions; automatic ones are
marked `(auto)`. A language without its own track is machine-translated,
which YouTube throttles (`rate_limited`: retry later or drop `--lang`).
`captions VIDEO` lists the tracks and the languages they translate into.

### playlist

The videos of any playlist, in playlist order.

### shorts, streams, community, releases, podcasts

Other tabs of a channel: Shorts, live streams (past and upcoming),
community posts, albums and singles, podcasts.

### channel-search

`channel-search CHANNEL QUERY` searches the videos of one channel.

### related, hashtag, suggest

`related VIDEO` lists the videos YouTube suggests next to it, `hashtag TAG`
the videos of a hashtag, `suggest PREFIX` what the search box suggests.

### rss

`rss CHANNEL` (or `rss PLAYLIST --playlist`): the 15 latest uploads with
exact publication times, views and likes, where listings only say "3 days
ago".

### accounts

The channels of the logged-in account (its own and brand accounts).

### listen

`listen VIDEO…` prints the live chat of streams as it comes (`-f jsonl`
for one object per message); super chats, stickers, memberships and gifts
carry their kind and `amount`. A stream that has ended is refused; read
its replay with `chat`.

### chat

`chat VIDEO` reads the replay of an ended stream in video order (each
message has its `offset` in seconds), or what a live chat shows now.

### send

`send VIDEO "text"` writes to the live chat of a stream that is live now
and that you can chat in. `--attach` and `--reply-to` are not supported.

### music

YouTube Music: `music search QUERY [-t KIND]` (`songs`, `videos`,
`albums`, `artists`, `playlists`, `podcasts`), `music tracks ID` (songs of
an album `MPREb…`, artist `UC…` or playlist, with plays),
`music lyrics VIDEO`.

### playlist-create, playlist-add, playlist-remove, playlist-edit, playlist-delete

Your own playlists:

- `playlist-create TITLE [VIDEO…] [--privacy] [--description]`, private
  by default;
- `playlist-add PLAYLIST VIDEO…`, `playlist-remove PLAYLIST VIDEO…`;
- `playlist-edit PLAYLIST [--title] [--description] [--privacy]`;
- `playlist-delete PLAYLIST [-y]` (asks first; `-y` in scripts).

### playlist-save

Saves someone's playlist to your library; `--undo` removes it.

### history-remove

Takes a video out of your watch history. It looks through the first
`--pages` pages of the history (5 by default).

### insights

With the OAuth setup (see Logging in), `atp yt insights [--days N]` and
`atp yt insights <your video>` report views, watch minutes, average view
duration and percentage, subscribers gained / lost, likes, dislikes,
comments, shares, playlist adds, daily series, and breakdowns by traffic
source, playback location, device, operating system, subscription status,
country, sharing service, age and gender (plus top videos for the
account). Days are counted in Pacific time, as YouTube Analytics counts
them. Impressions and click-through rate are not in the API.

Without the setup, a logged-in account gets its channel totals and the
current views of its uploads of the window, with a warning; any other
video gets its public counters.

### oauth

The one-time grant for `insights` and `edit`; see Logging in.

### edit

`edit VIDEO [TEXT] [--title TITLE] [--topic TAG ...] [--no-topics]
[--cover IMAGE] [--visibility public|private]` changes one of your videos
through the YouTube Data API; it needs the OAuth setup (see Logging in),
not the cookie session. It loads the video first, prints the change (each
field as old → new) and asks (`-y` in scripts).

- TEXT replaces the description, `--title` the title (100 characters at
  most), `--topic` the tags (`--no-topics` removes them; 500 characters
  in all). The rest of the video's details (category, languages,
  licence, embedding, made-for-kids) go back as they are, since the API
  deletes what an update leaves out.
- `--visibility public|private`; `friends` does not exist on YouTube
  and is refused. Making a scheduled video public drops its schedule.
  An unlisted video shows as `unlisted` in the summary.
- `--cover` sets a custom thumbnail (JPEG or PNG, 2 MB at most, 1280×720
  recommended); YouTube allows it only to channels verified by phone.
- Only videos of the channel the grant was made for can change. Each
  edit costs about 50 units of the project's daily API quota (10,000 by
  default), a thumbnail 50 more. Community posts cannot be edited here.

```
atp yt edit dQw4w9WgXcQ --title 'Better title' --topic music --topic 1987
atp yt edit https://youtu.be/dQw4w9WgXcQ - --visibility private -y < description.txt
atp yt edit dQw4w9WgXcQ --cover thumb.jpg
```

### download

The best streams come from YouTube's Android VR client in 8 MB ranges and
are merged with ffmpeg. Videos that need a proof-of-origin token (the
stream stops after 1 MB), and videos the Android client cannot play
(age-restricted, members-only, a bot check), are handed to `yt-dlp` when
it is installed, for plain https formats; without it they fail with a hint
to install it. Live streams can be downloaded once they end.

Downloads do not use the session: the Android client takes no cookies and
`yt-dlp` runs without them.

## Dates

Listings (search, channel tabs, playlists, feeds, comments) give ages such
as "3 weeks ago", not times. ATP turns an age of a day or more into a date
without a time of day (shown as `2026-07-16`, kept in JSON as midnight UTC
of that local date; months count as 30 days, years as 365), and an age in
hours or minutes into a time cut to that unit. Exact dates such as
`Oct 12, 2021` are kept as dates too. `read` gives the exact date of a
video where YouTube shows one, and `rss` exact publication times.

## Limits

- Requests are at least 1 s apart. Writes wait another 0.8–2 s and are
  never retried.
- YouTube's rate limit ends a command with `rate_limited`; wait a few
  minutes.
- Uploads and new community posts are not supported.
