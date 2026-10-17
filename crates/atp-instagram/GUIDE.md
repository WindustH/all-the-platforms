Instagram has two ways in: the web API with a browser session, for nearly
everything the site does, and the Graph API with an access token, for
`insights` and `ads`.

## Logging in

### Browser session

`atp instagram login` (or `login --browser`) imports the cookies of a
browser logged in at https://www.instagram.com; `--cookie 'sessionid=…;
csrftoken=…; ds_user_id=…'` takes the `Cookie` header of an
instagram.com request (`sessionid` is required, `csrftoken` is echoed in
a header, `ds_user_id` names your account). There is no password login.
`ATP_INSTAGRAM_COOKIE` supplies such a header for one run, without saving
it.

Without it, `user` and `read` work for public accounts (users by name, not
by numeric id), `comments` shows the first batch and `user-posts` the
latest twelve posts; Instagram may still answer some of it to logged-in
sessions only, depending on the network.

With the session: `user-posts` (all), `comments` (all) and `replies`,
`likers`, `followers`, `following`, `search -t user` / `-t topic`
(hashtags and places), `tag`, `location`, `stories`, `highlights`,
`highlight`, `favorites` (saved posts, `--folder` a collection),
`collections`, `likes`, `notifications`, `like`, `favorite`, `comment` /
`--reply-to`, `delete-comment`, `follow`, `delete` (your post), and direct
messages: `chats`, `messages`, `send`, `react`, `listen`. A command that
needs it fails with `not_authenticated` and the hint to run
`atp instagram login --browser`.

How long a session lasts is not known; when it ends, commands fail the
same way.

### Graph API token

`insights` and `ads` need an access token: `atp instagram login --token
TOKEN` checks it against the Graph API and saves it, or
`INSTAGRAM_ACCESS_TOKEN` / `META_ACCESS_TOKEN` supply one (they win over
the saved token). For insights it is an Instagram-login token (it starts
with `IG` and goes to graph.instagram.com) or a Facebook token of the Page
the business or creator account is linked to, with `instagram_basic` and
`instagram_manage_insights`. `INSTAGRAM_USER_ID` picks the account when
several are linked. `ads` always goes to graph.facebook.com, so it needs a
Facebook token (see `atp guide facebook ads`). An invalid or expired token
fails with `not_authenticated` and the hint to run `login --token` with a
new one or update the environment variable.

### Risk control

Instagram flags fast clients: requests are spaced at least 2.5 s apart. A
`rate_limited` answer ("Instagram says to wait a few minutes") means slow
down (`--interval 10`) and retry later. When Instagram asks to confirm the
account (a checkpoint), the command fails with `verification_required`:
open instagram.com in the browser, pass the check, and come back later (on
a terminal ATP opens it for you, see `atp guide login`).

## References

- Posts: a link (`/p/CODE/`, `/reel/CODE/`, `/tv/CODE/`), the shortcode, or
  the numeric media id.
- Users: `name`, `@name`, a profile link, or the numeric id (logged in).
- Hashtags: `#tag` or `tag`; places: the id or an `/explore/locations/` link.
- Highlights: the id `highlights` lists (`highlight:ID` works too), or `#N`
  after it.
- Direct messages: chats are thread ids, messages `THREAD/ITEM`.

## Commands

### search

`-t user` finds accounts, `-t topic` hashtags, then places. There is no
post search.

### tag, location

`tag TAG` and `location PLACE` list top posts; `--recent` the newest.

### stories, highlights, highlight

`stories USER` shows current stories, `highlights USER` the story
highlights, `highlight ID` the stories inside one.

### favorites, collections, likes

Your own only: saved posts (`--folder` a collection from `collections`)
and liked posts.

### comment

`--reply-to` answers a comment; `comment --image` is not supported.

### edit

`edit POST TEXT` replaces the caption of one of your posts (photos,
carousels, reels), as the web app's post editor does; it needs the
browser session. It loads the post first, prints the change (the caption
as old → new, with what it keeps) and asks (`-y` in scripts).

- Hashtags and mentions are written in the caption.
- Only the caption changes: the location, tagged people, collaborators
  and alt texts are not sent, so they stay as they are.
- Posts promoted with ads running cannot change until the promotion
  ends (`permission_denied`).

```
atp instagram edit https://www.instagram.com/p/C1a2b3c4d5e/ 'New caption #travel'
```

### send, react

`send THREAD TEXT [--reply-to THREAD/ITEM]`: text only (`--attach` is not
supported). `react THREAD/ITEM EMOJI [--undo]` reacts to a message.

### listen

Polls the direct inbox every 20 s and prints new messages of the threads
you name (all when none).

### insights

Business and creator accounts only, through the Graph API (see Logging
in).

- account (`--days`, at most 30): views, reach, accounts engaged,
  interactions, likes, comments, shares, saves, follows, profile link taps;
  daily reach and new followers; follower age, gender, country and city.
- post: views, reach, likes, comments, shares, saves, profile visits,
  follows, and reel watch times. A post given by link is looked up among the
  account's latest 500.

### ads

The Marketing API with a Facebook token (same commands as
`atp facebook ads`): accounts, campaigns, ad sets, ads, creatives,
audiences, pixels, insights reports and `raw`.

## Limits

Uploading posts is not supported, nor are pictures in comments and
attachments in direct messages.
