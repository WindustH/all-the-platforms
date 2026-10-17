Facebook has two ways in: **facebook.com**, read the way a browser sees
it, and **the Graph API**, for the Pages an access token manages and for
ads. They log in separately, and one setup may hold both.

## Logging in

### facebook.com session

`atp facebook login` (or `login --browser`) imports the cookies of a
browser logged in at https://www.facebook.com; `--cookie 'c_user=…;
xs=…'` takes the `Cookie` header of a facebook.com request, which must
contain `c_user` and `xs`. `ATP_FACEBOOK_COOKIE` supplies such a header for
one run, without saving it.

It serves `user`, `user-posts`, `read`, `comments`, `group`,
`group-posts` and `download`. Logged out, Facebook shows profiles and
Pages, posts with their reactions by type, the first comments, and public
groups with their discussion; it refuses the rest (timelines past the
newest post, more comments) and cuts off groups after a few dozen
requests. With a session those continue, and `feed`, `search`,
`search -t user`, `notifications` and `friends` work.

### Graph API token

`atp facebook login --token TOKEN` checks an access token against the
Graph API and saves it; `FACEBOOK_ACCESS_TOKEN` or `META_ACCESS_TOKEN`
supply one instead (they win over the saved token). Take a user or Page token from
the Graph API Explorer or a Meta app. A user token that manages Pages
needs the permissions `pages_show_list`, `pages_read_engagement`,
`pages_manage_posts`, `pages_manage_engagement`, `pages_messaging` and
`read_insights`; `ads` needs `ads_read` / `ads_management`. A call the token has no permission for fails with
`permission_denied` (check its scopes and the app's review status).

The token serves `pages`, `use-page`, `page-posts`, `post`, `edit`,
`delete`, `comment`, `delete-comment`, `comments` and `replies` of Page posts,
`insights`, `chats`, `messages`, `send` and `ads`. With a user token,
ATP acts on a Page with that Page's own token, looked up with yours.

The Page to act on is `--topic` / the argument, `FACEBOOK_PAGE_ID`, the
one saved by `use-page`, or the only one the token manages.
`META_GRAPH_VERSION` picks another Graph API version than the default
`v24.0`.

### When a session or token ends

How long a facebook.com session lasts is not known. When it is gone, the
reads that need one fail with `not_authenticated` ("needs a logged-in
Facebook session"), and paged lists stop with a note to log in. How long
a token lasts depends on how it was made; an invalid or expired one fails
with `not_authenticated` and the hint to run `login --token` with a new
one or update the environment variable.

### Risk control

Facebook's security interstitial ("blocked by our security systems")
comes back as `verification_required`: wait a while, or log in. On a
terminal ATP opens facebook.com in a browser for it (see `atp guide
login`). Logged-out reads are rate limited by IP: keep them slow; requests
are spaced at least 1.5 seconds apart. The Graph API's rate limits come
back as `rate_limited` (usually lifted within an hour), a policy block
(code 368) as `permission_denied`; usage above 90 % of a limit is
logged as a warning.

## References

- Profiles and Pages: a handle (`nasa`, `@nasa`), a numeric id, or a link.
- Posts: the permalink (`…/posts/…`, `permalink.php?story_fbid=…&id=…`,
  photos, videos, reels, `fb.watch/…`, `/share/p/…`) or `PROFILE/POST_ID`;
  Facebook has no route for a bare post id. Page posts listed by
  `page-posts` carry Graph ids (`PAGEID_POSTID`), which `read`, `comments`,
  `comment`, `delete` and `insights` take.
- Comments of Page posts: their Graph ids (like `123_456`) from `comments`.
- Groups: an id, a slug or a link.
- Messenger conversations: the id (`t_…`) `chats` lists; `send` also
  takes a person's Page-scoped id (PSID).

## Commands

### read, comments

A link reads through facebook.com, a Graph id (`PAGEID_POSTID`) through
the Graph API; there `comments` lists the top-level comments, newest
first.

### replies

Only for comments of Page posts, by Graph id.

### feed, search, notifications, friends

Need a facebook.com session. `search -t user` finds people and Pages.

### group, group-posts

`group` shows a group's name, privacy, members and description;
`group-posts` its discussion.

### pages, use-page, page-posts

`pages` lists the Pages your token manages; `use-page ID` makes one the
default; `page-posts [PAGE]` lists a Page's posts with the Graph ids the
other commands take.

### post

`post TEXT [-i IMAGE …] [--topic PAGE]` publishes to the Page; a title goes
above the text, and several images become one post. `--reply-to` and
`--quote` are not supported (use `comment`).

### comment, delete-comment

On Page posts, by Graph id: `comment POST TEXT [--reply-to COMMENT]`
(`-i` attaches a photo), `delete-comment POST COMMENT`.

### edit

`edit POST TEXT` replaces the text of a Page post, by Graph id
(`PAGE_POST`, as `page-posts` lists them), through the Graph API as the
Page. It loads the post first, prints the change (the text as old → new)
and asks (`-y` in scripts). Photos, links and the post's other settings
stay as they are.

The Graph API lets an app edit only the Page posts that app published:
posts made with `atp facebook post` under the same token's app can
change; posts written on facebook.com, in the Meta Business Suite or with
another app's token cannot, and fail with `permission_denied` or
`invalid_input` and a hint saying so. The token needs
`pages_manage_posts`. Posts on a personal profile cannot be edited here.

```
atp facebook edit 1234567890_9876543210 'Opening hours: 9 to 5 from Monday'
```

### delete

Deletes a Page post by Graph id.

### insights

`insights` gives the Page's daily metrics over `--days` (at most 90) with
its followers and likes; `insights POST` the lifetime metrics of a Page
post, with reactions by type. Metrics are asked one by one, and names Meta
has retired show up in `warnings`.

### chats, messages, send

The Page's Messenger inbox: `chats`, `messages CONVERSATION`, `send
CONVERSATION TEXT` (`--reply-to` answers one message). Replies only within
Meta's 24-hour window; no attachments (send a link instead).

### ads

`ads` drives the Marketing API with the same token: `me`, `accounts`, `use
ACCOUNT`, `account`, `campaigns`, `adsets`, `ads`, `creatives`,
`audiences`, `pixels`, `get ID`, `create-campaign` (paused unless
`--status ACTIVE`), `update ID key=value…`, `pause`, `activate`, `delete
-y`, `insights` (rows by level, date preset or range, breakdowns, daily
steps) and `raw PATH key=value…` for anything else. The ad account: `-a`,
`META_AD_ACCOUNT_ID`, or the one saved by `ads use`. Budgets are in the
account currency's minor unit (cents).

## Limits

Facebook changes its page structure often; when a read comes back empty,
`--raw` shows what the page carried.
