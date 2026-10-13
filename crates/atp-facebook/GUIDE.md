## References

- Profiles and Pages: a handle (`nasa`, `@nasa`), a numeric id, or a link.
- Posts: the permalink (`…/posts/…`, `permalink.php?story_fbid=…&id=…`,
  photos, videos, reels, `fb.watch/…`, `/share/p/…`) or `PROFILE/POST_ID`;
  Facebook has no route for a bare post id. Page posts listed by
  `page-posts` carry Graph ids (`PAGEID_POSTID`), which `read`, `comments`,
  `comment`, `delete` and `insights` take.
- Groups: an id, a slug or a link.

## Two ways in

**facebook.com** (`user`, `user-posts`, `read`, `comments`, `group`,
`group-posts`, `download`) reads what the site ships to a browser.
Logged out it shows profiles and Pages, posts with their reactions by type,
the first comments, and public groups with their discussion; Facebook
refuses the rest (timelines past the newest post, more comments) and cuts
off groups after a few dozen requests. With a browser session
(`login --browser`, or `--cookie 'c_user=…; xs=…'`) those continue, and
`feed`, `search`, `search -t user`, `notifications` and `friends` work.

**The Graph API** needs an access token: `login --token TOKEN`, or
FACEBOOK_ACCESS_TOKEN / META_ACCESS_TOKEN. With a user token that manages
Pages (permissions pages_show_list, pages_read_engagement,
pages_manage_posts, pages_manage_engagement, pages_messaging,
read_insights):

- `pages`, `use-page ID`, `page-posts [PAGE]`;
- `post TEXT [-i IMAGE …] [--topic PAGE]` publishes to the Page (several
  images become one post), `delete`, `comment` / `--reply-to`
  (`-i` attaches a photo), `delete-comment`, `comments` / `replies` of
  Page posts;
- `insights` (daily Page metrics over `--days`, at most 90) and
  `insights POST` (lifetime post metrics, reactions by type);
- `chats`, `messages CONVERSATION`, `send CONVERSATION TEXT`: the Page's
  Messenger inbox (replies only within Meta's 24-hour window).

The Page is `--topic` / the argument, FACEBOOK_PAGE_ID, the one saved by
`use-page`, or the only one the token manages. Metric names Meta has
retired show up in `warnings`.

## Ads

`ads` drives the Marketing API with the same token (ads_read /
ads_management): `accounts`, `use ACCOUNT`, `campaigns`, `adsets`, `ads`,
`creatives`, `audiences`, `pixels`, `get ID`, `create-campaign`, `update ID
key=value…`, `pause`, `activate`, `delete -y`, `insights` (rows by level,
date preset or range, breakdowns, daily steps) and `raw PATH key=value…` for
anything else. The ad account: `-a`, META_AD_ACCOUNT_ID, or `ads use`.
META_GRAPH_VERSION picks another Graph API version.

## Limits

Facebook changes its page structure often; when a read comes back empty,
`--raw` shows what the page carried. Logged-out reads are rate limited by
IP: keep them slow.
