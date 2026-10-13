## References

- Articles: `https://mp.weixin.qq.com/s/<id>` or the long
  `https://mp.weixin.qq.com/s?__biz=…&mid=…&idx=…&sn=…&chksm=…` form. Keep
  `chksm` on long links: without it WeChat shows a captcha. Lists give
  articles as `BIZUIN_MID_IDX`, which `comments` and `insights` take too.
- Accounts: an article or album link, the account's fakeid (its `__biz`,
  e.g. `MzI5MjQyOTEyNg==`), or a name / WeChat ID to search (backend).
- Albums (合集): an `mp/appmsgalbum?__biz=…&album_id=…` link, or `BIZ/ALBUM_ID`.
- Drafts: a number (backend `appMsgId`) or an API `media_id`.

## Three ways in

1. **Public pages, no login**: `read` and `download` any article (text,
   account, time, original-size images, article videos, audio) and `album`
   lists. Read / like counts and comments of others' articles are not public.
2. **Backend (QR login)**: `atp mp login` shows a QR code; scan it with
   the WeChat of an admin or operator of the account. Works for every
   account type: drafts, publishing (发表 or 群发), your published articles
   with read / like / 在看 / share / comment counts, other accounts'
   articles (`user-posts NAME`), comments, follower and content analytics.
   Sessions last a few days; log in again when commands report it expired.
3. **Official API (AppID + AppSecret)**: `atp mp login --token
   APPID:SECRET`, or `WECHAT_APPID` / `WECHAT_SECRET` in the environment
   (they win). Get both at 微信开发者平台 → 我的业务 → 公众号 → 基础信息 →
   开发密钥, and add this machine's public IP to the API IP whitelist there
   (otherwise every call fails with "IP not on the API whitelist").

What the API allows depends on the account (per WeChat's docs):

| | personal / unverified | verified organization |
| --- | :-: | :-: |
| drafts, material, menu / auto-reply (read) | ✓ | ✓ |
| publish, list and delete published posts | ✗ | ✓ |
| comments, analytics (datacube), followers, tags | ✗ | ✓ |

On publishing WeChat says: "2025年7月起，个人主体账号、企业主体未认证账号及不支持
认证的账号将被回收以上接口的调用权限" (the freepublish API). Personal accounts
publish through the backend instead. Even verified accounts cannot list
posts sent with notification (群发) through the API.

With both routes set up, each command picks the one that can do the job:
the API for drafts and material, the backend for your published list,
comments, analytics and publishing backend drafts.

## Commands

- `read`, `download`, `album ALBUM`, `user ACCOUNT`, `search -t user NAME`
  (backend), `user-posts [ACCOUNT]` (yours by default; backend for others,
  20 per page, rate limited after a few hundred).
- Drafts: `post TEXT --title T -i IMAGE` saves a draft (the text is HTML
  when it starts with a tag, else one paragraph per line; local images in
  the HTML are uploaded; the first image is the cover). `draft-new` takes
  `--author`, `--digest`, `--source-url`, `--cover`, `--no-comments`,
  `--fans-only`. `drafts`, `draft ID`, `draft-count`, `draft-delete ID`.
  API drafts need a cover image and titles of at most 32 characters.
- `publish DRAFT [--notify]`: without `--notify` it publishes (发表, no push);
  with it, it sends to followers (群发, once a day for subscription
  accounts). The backend may show a QR code for the admin to confirm.
  `delete ARTICLE` deletes a published article (API, verified accounts).
- Comments: `comments ARTICLE`, `latest-comments` (backend);
  `comment ARTICLE TEXT --reply-to COMMENT`, `delete-comment`,
  `delete-reply`, `elect [--undo]` (精选), `open-comments [--close]` (API,
  verified accounts). Accounts cannot write top-level comments.
- `insights [--days N]`: daily reads, shares, likes, 在看, comments,
  favorites, read sources, follower gains / losses / total. `insights
  ARTICLE`: lifetime counts (backend) and the daily detail of its first 30
  days with completion rate and reading time (API, articles since
  2025-11-01). Days end yesterday, China time; a day's data is ready after
  08:00 the next morning.
- `followers`, `tags`, `tagged TAG`, `blacklist` (API, verified accounts;
  WeChat no longer reveals follower nicknames).
- `material [-t image|video|voice|news]`, `material-count`,
  `material-upload FILE`, `material-delete ID`; `menu`, `autoreply`,
  `quota /cgi-bin/draft/add`, `rid RID` (API); `accounts` (backend).

## Limits

Article pages are fetched as Safari: WeChat shows desktop Chrome a captcha
("环境异常"). The backend endpoints are unofficial and can change; listing
another account's history quickly is rate limited (`rate_limited`: stop for
an hour or more).
