## References

- Videos and photo posts: `https://www.tiktok.com/@user/video/<id>`,
  `…/photo/<id>`, short links (`https://vm.tiktok.com/…`,
  `https://vt.tiktok.com/…`, `https://www.tiktok.com/t/…`) or a bare id.
- Users: `tiktok`, `@tiktok`, a profile link, or a secUid (`MS4wLjAB…`).
- Hashtags: `#name`, `name` or a `/tag/` link; sounds and playlists: their
  link or id.

## Logging in

`social tiktok login` shows a QR code for the TikTok app (Profile → ☰ →
the scan icon). `login --browser` reuses a browser session, `--cookie`
takes a cookie header with `sessionid`.

The Marketing API (`ads`) uses an access token instead:
`social tiktok login --token <token>` or `TIKTOK_ADS_ACCESS_TOKEN`; listing
the ad accounts a token manages also needs `TIKTOK_ADS_APP_ID` and
`TIKTOK_ADS_SECRET`.

## What works without a login

Pages: `read` (videos and photo posts, with exact counters, the stream
ladder and the sound), `user`, `insights` of a post, and `download`. TikTok
puts a proof-of-work check in front of pages now and then; it is solved like
the browser does.

Lists come from the web client's signed API (`X-Bogus`, `msToken`):
comments, a user's videos, search, the recommended feed, followers,
hashtag / sound / playlist videos. TikTok scores these calls and often
answers anonymous ones with nothing (`verification_required`); a logged-in
session from a residential network fares best. Newer web pages also sign
with `X-Gnarly`, which is not reproduced.

## Analytics

`insights` reports the public counters: for a post views, likes, comments,
shares, favorites, reposts and the like rate; for your account followers,
following, likes and posts. TikTok Studio's analytics are not reachable
through the web API.

Ad accounts: `ads report [--level campaign|adgroup|ad|advertiser]
[--dimensions …] [--metrics …] [--from … --to … | --days N]`, one row per
dimension combination (`-f csv` for spreadsheets); `ads campaigns`,
`adgroups`, `ads`, `audiences`, `pixels`, `advertiser`, and `ads raw PATH
key=value …` for any other Marketing API endpoint. `ads use ID` sets the
default ad account (or `-a ID`, `TIKTOK_ADS_ADVERTISER_ID`).

## Interactions

- `tag NAME`, `sound ID [--info]`, `playlist ID [--info]`, `playlists
  [USER]`, `related VIDEO`.
- `search -t topic NAME` looks up that exact hashtag (the web search has no
  hashtag tab); `search --filter video` searches videos only.
- `collections` are your favorites folders; `favorites --folder ID` lists
  one.
- `like`, `favorite`, `comment [--reply-to COMMENT]`, `delete-comment`,
  `follow` and `delete` need a login.

## Downloads

The highest-resolution stream without watermark (HEVC when TikTok has it),
photo posts' images and their sound; `--audio-only` keeps the audio.
