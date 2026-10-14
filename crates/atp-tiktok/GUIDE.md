## Logging in

`atp tiktok login` shows a QR code: in the TikTok app open Profile → ☰ →
the scan icon, scan it and confirm. The session cookies arrive with the
confirmation. When TikTok confirms the scan but sets no session, the login
fails with `not_authenticated`; when TikTok rate-limits the QR login, with
`rate_limited` (wait a while). Use `--browser` or `--cookie` then.

`login --browser` reads the session of a browser logged in at tiktok.com.
With `--cookie`, copy the whole `Cookie` header of a request to
`www.tiktok.com`: `sessionid` is the session, writes also send
`tt_csrf_token`, and API calls reuse the `msToken` cookie when there is
one.

`login --token ACCESS_TOKEN` keeps an access token of the official
Marketing API with the session, for `ads` only: a long-term token of a
Marketing API app from https://business-api.tiktok.com (a token has no
spaces). It does not log in to tiktok.com. A session with a token and no
cookies counts as logged in: `login` checks the token by asking whom it
belongs to, and `whoami` shows that person (`source: marketing-api`).
Listing the ad accounts a token manages also needs the app's id and
secret (`TIKTOK_ADS_APP_ID`, `TIKTOK_ADS_SECRET`).

### What works without a login

Pages: `read` (videos and photo posts, with exact counters, the stream
ladder and the sound), `user`, `insights` of a post, and `download`.
TikTok puts a proof-of-work check in front of pages now and then; it is
solved like the browser does.

Lists come from the web client's signed API (`X-Bogus`, `msToken`):
comments, a user's videos, search, the recommended feed, followers,
hashtag / sound / playlist videos. TikTok scores these calls and often
answers anonymous ones with nothing (`verification_required`); a logged-in
session from a residential network fares best. Newer web pages also sign
with `X-Gnarly`, which is not reproduced.

Your own likes, favorites, collections and playlists (without a USER),
account `insights`, and every write (`like`, `favorite`, `comment`,
`delete-comment`, `follow`, `delete`) need a cookie session.

### When the session ends

How long a session lasts is not known. When TikTok stops accepting it,
commands fail with `not_authenticated` ("TikTok did not recognize the
session", "TikTok wants a login for this"); log in again. An invalid or
expired Marketing API token fails `ads` with `not_authenticated` as well.

### Bot checks

- When TikTok keeps serving its proof-of-work page, the command fails with
  `verification_required`: open tiktok.com in a browser, or retry later /
  through `--proxy`.
- An empty answer from the API, or TikTok's codes 10000 / 10101, also mean
  `verification_required`: log in (`--browser`), or retry later / from
  another network (`--proxy`).

### Environment

- `TIKTOK_ADS_ACCESS_TOKEN`: the Marketing API access token; it wins over
  the one `login --token` saved.
- `TIKTOK_ADS_APP_ID`, `TIKTOK_ADS_SECRET`: the Marketing API app's id and
  secret, for `ads advertisers`.
- `TIKTOK_ADS_ADVERTISER_ID`: the ad account to use when `-a` is not
  given (before the one `ads use` saved).
- `ATP_TIKTOK_COOKIE`: a cookie header for one run, never saved.

## References

- Videos and photo posts: `https://www.tiktok.com/@user/video/<id>`,
  `…/photo/<id>`, short links (`https://vm.tiktok.com/…`,
  `https://vt.tiktok.com/…`, `https://www.tiktok.com/t/…`) or a bare id.
- Users: `tiktok`, `@tiktok`, a profile link (short links too), or a
  secUid (`MS4wLjAB…`).
- Hashtags: `#name`, `name` or a `/tag/` link.
- Sounds: the id or a `/music/<name>-<id>` link.
- Playlists: the id or a `/@user/playlist/<name>-<id>` link.
- Favorites collections: the id or a `/@user/collection/<name>-<id>` link.
- Comments: the comment id.

## Commands

### search

`--filter general` (the Top tab, the default) or `--filter video` (videos
only). `-t user` finds accounts. `-t topic NAME` looks up that exact
hashtag, with its video and view counts (the web search has no hashtag
tab).

### hot, feed

Both read the recommended "For You" feed, personal when logged in. It has
no end; any cursor continues it.

### read

Videos and photo posts from their page, with exact counters, the stream
ladder and the sound; no login needed.

### likes

Videos a user liked, public only when the user allows it; yours without a
USER.

### favorites, collections

`favorites` lists saved videos (yours without a USER); `--folder ID` lists
one favorites collection (its id or link). `collections` are the favorites
folders.

### favorite, unfavorite

Saves to (or removes from) your saved videos; `--folder` is not supported.

### comment, delete-comment

`--reply-to COMMENT` takes the id of the comment to answer.

### delete

Deletes one of your posts.

### tag

`tag NAME`: the videos of a hashtag.

### sound

`sound ID`: the videos using a sound; `--info` shows the sound itself.

### playlist

`playlist ID`: the videos of a creator's playlist; `--info` shows the
playlist itself.

### playlists

`playlists [USER]`: the playlists (mixes) a creator made of their videos;
yours without a USER.

### related

`related VIDEO`: the videos TikTok recommends next to one.

### insights

The public counters: for a post views, likes, comments, shares,
favorites, reposts and the like rate; for your account followers,
following, likes, posts and friends. TikTok Studio's analytics (traffic
sources, audience, watch time) are not reachable through the web API.
Ad accounts have their reports in `ads report`.

### ads

The official Marketing API, with the token of `login --token` or
`TIKTOK_ADS_ACCESS_TOKEN`; no cookie session needed. The commands that
work on one ad account take `-a ID`, else `TIKTOK_ADS_ADVERTISER_ID`, else
the one `ads use ID` saved. Lists come as rows, one per object (`-f csv`
for spreadsheets); `-n` caps them (100 by default, 0 for all).
`--status` keeps the campaigns, ad groups or ads of one primary status.

### ads me, ads advertisers, ads advertiser, ads use

`me` is the person the token belongs to. `advertisers` lists the ad
accounts the token manages (needs `TIKTOK_ADS_APP_ID` and
`TIKTOK_ADS_SECRET`). `advertiser [ID…]` shows details (the current
account by default). `use ID` saves the default ad account with the
session.

### ads campaigns, ads adgroups, ads ads, ads audiences, ads pixels

Campaigns, ad groups (`--campaign ID`, repeatable), ads (`--campaign ID`,
`--adgroup ID`), custom and lookalike audiences, and pixels (the first 20)
of the ad account.

### ads report

`ads report` gives one row per dimension combination, dimensions then
metrics. `--level` is `campaign` (the default), `adgroup`, `ad` or
`advertiser`. The dimensions default to `<level>_id,stat_time_day`. The
window is `--from … --to …` (YYYY-MM-DD), or the `--days N` days (7)
up to yesterday. `--type` picks `BASIC`, `AUDIENCE`, `PLAYABLE_MATERIAL`
or `CATALOG`, `--filter` takes the API's filtering JSON. Dimension and
metric names are the Marketing API's (`report/integrated/get`).

### ads raw

`ads raw PATH key=value …` calls any other Marketing API endpoint (the
path after `/open_api/v1.3`); values that parse as JSON are sent as JSON,
`--post` sends a JSON body.

### download

The highest-resolution stream without watermark (HEVC when TikTok has
it), photo posts' images and their sound; `--audio-only` keeps the audio.
Streams are bound to the `tt_chain_token` cookie the post's page set; ATP
sends it along. No login needed.

## Limits

- Requests are at least 1.5 s apart; writes wait another 1.5–4 s, as a
  person would.
- TikTok's rate limit ends a command with `rate_limited` ("TikTok is rate
  limiting"); wait a while.
- Uploads are not supported.
