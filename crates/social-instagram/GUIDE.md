## References

- Posts: a link (`/p/CODE/`, `/reel/CODE/`, `/tv/CODE/`), the shortcode, or
  the numeric media id.
- Users: `name`, `@name`, a profile link, or the numeric id (logged in).
- Hashtags: `#tag` or `tag`; places: the id or an `/explore/locations/` link.
- Direct messages: chats are thread ids, messages `THREAD/ITEM`.

## Logging in

`social instagram login --browser` (or `--cookie 'sessionid=…;
csrftoken=…; ds_user_id=…'`) reuses a browser session; there is no password
login. Without it, `user` and `read` work for public accounts, `comments`
shows the first batch and `user-posts` the latest twelve posts.

With the session: `user-posts` (all), `comments` (all) and `replies`,
`likers`, `followers`, `following`, `search -t user` / `-t topic`
(hashtags and places), `tag`, `location`, `stories`, `highlights`,
`highlight`, `favorites` (saved posts, `--folder` a collection),
`collections`, `likes`, `notifications`, `like`, `favorite`, `comment` /
`--reply-to`, `delete-comment`, `follow`, `delete` (your post), and direct
messages: `chats`, `messages`, `send`, `react`, `listen` (polls every 20 s).
Uploading posts is not supported.

Instagram flags fast clients: requests are spaced at least 2.5 s apart. A
`rate_limited` or `verification_required` answer means stop, open the site,
and come back later.

## Insights

`insights` and `insights POST` use the Graph API for business and creator
accounts: an Instagram-login token (`IG…`) or a Facebook token of the Page
the account is linked to (instagram_basic, instagram_manage_insights), via
`login --token` or INSTAGRAM_ACCESS_TOKEN / META_ACCESS_TOKEN.
INSTAGRAM_USER_ID picks the account when several are linked.

- account (`--days`, at most 30): views, reach, accounts engaged,
  interactions, likes, comments, shares, saves, follows, profile link taps;
  daily reach and new followers; follower age, gender, country and city.
- post: views, reach, likes, comments, shares, saves, profile visits,
  follows, and reel watch times. A post given by link is looked up among the
  account's latest 500.

## Ads

`ads` is the Marketing API with a Meta token (same commands as
`social facebook ads`): accounts, campaigns, ad sets, ads, creatives,
audiences, pixels, insights reports and `raw`.
