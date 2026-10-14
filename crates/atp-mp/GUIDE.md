## Logging in

There are three ways in, by what they reach; the backend and the API can
be set up together.

### Public pages, no login

`read` and `download` any article (text, account, time, original-size
images, article videos, audio), `album` lists, and `user` from an article
or album link. Read / like counts and comments of others' articles are not
public.

### Backend (QR code or browser session)

`atp mp login` shows the QR code of the mp.weixin.qq.com login page; scan
it with the WeChat of an admin or operator of the account and confirm.
This works for every account type: drafts, publishing (发表 or 群发), your
published articles with read / like / 在看 / share / comment counts, other
accounts' articles (`user-posts NAME`), account search, comments, follower
and content analytics.

- A WeChat user who administers no official account it can log in to is
  refused at the scan.
- WeChat refuses the QR login for an account without a bound email; bind
  one in the mp.weixin.qq.com settings and log in again.
- `atp mp login --browser` imports the session of a browser logged in at
  https://mp.weixin.qq.com instead, and `--cookie '...'` takes the
  `Cookie` header of any mp.weixin.qq.com request; the `slave_sid` cookie
  is required. `ATP_MP_COOKIE` supplies such a header for one run.

Backend sessions last a few days. When one ends, commands fail with
`not_authenticated` ("the mp.weixin.qq.com session has expired"); log in
again.

### Official API (AppID + AppSecret)

1. At 微信开发者平台 → 我的业务 → 公众号 → 基础信息 → 开发密钥, take the
   AppID (it starts with `wx`) and the AppSecret.
2. Add this machine's public IP to the API IP whitelist there. Otherwise
   every call fails with `ip_blocked` ("IP not on the API whitelist"),
   naming the address to add.
3. Run `atp mp login --token APPID:SECRET`.

`WECHAT_APPID` and `WECHAT_SECRET` in the environment work too and win
over the saved pair. `login` and `whoami` check the pair with a call.
Access tokens come from `stable_token`, so parallel runs do not invalidate
each other; ATP caches them privately and renews them before they expire.
The AppID and AppSecret do not expire here; when WeChat refuses them,
commands fail with `not_authenticated`.

Without the whitelist, WeChat may instead ask the account admin to confirm
calls from a new IP: the command fails with `verification_required`;
approve the message the admin received and retry from the same IP. When
the admin rejects it, calls fail with `ip_blocked`: wait (1 h after a first
rejection, 24 h after repeated ones) or use the whitelist.

What the API allows depends on the account (per WeChat's docs):

| | personal / unverified | verified organization |
| --- | :-: | :-: |
| drafts, material, menu / auto-reply (read) | ✓ | ✓ |
| publish, list and delete published posts | ✗ | ✓ |
| comments, analytics (datacube), followers, tags | ✗ | ✓ |

On publishing WeChat says: "2025年7月起，个人主体账号、企业主体未认证账号及不支持
认证的账号将被回收以上接口的调用权限" (the freepublish API). Personal accounts
publish through the backend instead. Even verified accounts cannot list
posts sent with notification (群发) through the API. A call the account may
not make fails with `permission_denied` and says which route to use.

### Which route a command takes

With both routes set up, each command picks the one that can do the job:
the API for drafts and material, the backend for your published list,
comments, analytics and publishing backend drafts. `whoami` shows the
`route` it used and, when both are set up, the `appid` or the API's error
(`api_error`).

## References

- Articles: `https://mp.weixin.qq.com/s/<id>` or the long
  `https://mp.weixin.qq.com/s?__biz=…&mid=…&idx=…&sn=…&chksm=…` form. Keep
  `chksm` on long links: without it WeChat shows a captcha. Lists give
  articles as `BIZUIN_MID_IDX`, which `comments` and `insights` take too;
  your published list through the API gives `ARTICLE_ID/INDEX`.
- Accounts: an article or album link, the account's fakeid (its `__biz`,
  e.g. `MzI5MjQyOTEyNg==`), or a name / WeChat ID to search (backend).
  Empty, `me`, your `gh_` id or your AppID mean your own account.
- Albums (合集): an `mp/appmsgalbum?__biz=…&album_id=…` link, or `BIZ/ALBUM_ID`.
- Drafts: a number (backend `appMsgId`) or an API `media_id`.

## Commands

### read, download

Public article pages, no login needed. With a backend login, `read` adds
the counts of your own articles.

### album

`album ALBUM`: the articles of an album (合集), the one anonymous listing of
an account's articles.

### user

`user ACCOUNT`: a link needs no login; a name or WeChat ID is searched in
the backend; your own account shows what `whoami` shows.

### search

`search -t user NAME` finds accounts by name or WeChat ID (backend, five per
page). Accounts can opt out of being found by name. There is no article
search.

### user-posts

`user-posts [ACCOUNT]`, yours by default: through the backend with counts,
else through the API (verified accounts, only posts published without
notification). Others' articles need the backend: 20 per page, rate
limited after a few hundred articles (`rate_limited`: stop for an hour or
more).

### followers, tags, tagged, blacklist

API, verified accounts, your own account only. WeChat no longer reveals
follower nicknames; follow time, scene, remark and tags remain.

### post

`post TEXT --title T -i IMAGE` saves a draft; `publish` publishes it. The
text is HTML when it starts with a tag, else one paragraph per line; local
images in the HTML (`<img src="shot.png">`) are uploaded; the first image
is the cover. `--reply-to` and `--quote` are not supported.

Drafts go through the API whenever API credentials are set, else through
the backend. API drafts need a cover image, titles of at most 32
characters, and body images in JPEG or PNG under 1 MB.

### draft-new

Saves a draft with every option: `--author`, `--digest`, `--source-url`,
`--cover`, `--no-comments`, `--fans-only`.

### drafts, draft, draft-count, draft-delete

The draft box, one draft with its text, the count (API) and deleting. A
numeric id is a backend draft, anything else an API `media_id`.

### publish

`publish DRAFT [--notify]`: without `--notify` it publishes (发表, no push);
with it, it sends to followers (群发, once a day for subscription accounts).
A backend draft is published through the backend, which may show a QR code
for the admin to confirm (安全验证; ATP waits up to three minutes). An API
draft goes through the API: verified organization accounts only, and never
with `--notify`.

### delete

`delete ARTICLE` deletes a published article (API, verified accounts), by
`ARTICLE_ID[/INDEX]` or by its link, looked up among the API's published
list (posts sent with notification are not there).

### comments, latest-comments

`comments ARTICLE` reads through the backend (your recent articles) or the
API (verified accounts). `latest-comments` (backend) lists your articles
with their newest comments.

### comment, delete-comment, delete-reply, elect, open-comments

API, verified accounts with the 留言 feature. Accounts cannot write
top-level comments: `comment ARTICLE TEXT --reply-to COMMENT` answers a
reader (one reply per comment). `delete-comment`, `delete-reply`,
`elect [--undo]` (精选) and `open-comments [--close]` act on an article's
comments.

### insights

`insights [--days N]`: daily reads, shares, likes, 在看, comments,
favorites, read sources, follower gains / losses / total. `insights
ARTICLE`: lifetime counts (backend) and the daily detail of its first 30
days with completion rate and reading time (API, articles since
2025-11-01). Days end yesterday, China time; a day's data is ready after
08:00 the next morning. Each route adds what it can; one that fails leaves
a note instead of an error.

### material, material-count, material-upload, material-delete

The material library (素材) through the API, for every account type.
Without API credentials `material` lists the backend's image library
(images only).

### menu, autoreply, quota, rid

API: the menu and auto-replies as configured, the daily quota of an API
path (`quota /cgi-bin/draft/add`), and a failed request looked up by the
`rid` in its error. API quota errors come back as `rate_limited`.

### accounts

Backend: the accounts this session can switch to.

## Limits

- Article pages are fetched as Safari: WeChat shows desktop Chrome a
  captcha ("环境异常"), which comes back as `verification_required`.
- The backend endpoints are unofficial and can change; listing another
  account's history quickly is rate limited (`rate_limited`: stop for an
  hour or more; long lists trip it after about 600 articles).
- ATP keeps at least 1.5 s between requests.
