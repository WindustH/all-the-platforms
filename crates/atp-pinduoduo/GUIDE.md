## Logging in

Pinduoduo has two routes, and either one counts as logged in: the
official 多多进宝 open API with an app's credentials, and your own login on
the H5 site mobile.yangkeduo.com. There is no QR login (the web login is
phone and SMS only), so `atp pdd login` without a flag imports the H5
session from a browser.

### 多多进宝 (official open API)

1. Create an app of the 多多进宝 type on https://open.pinduoduo.com; it
   has a client id and a client secret.
2. Create a 推广位 (a pid) on https://jinbao.pinduoduo.com. PDD wants the
   pid authorized (备案) once before searches.
3. Run `atp pdd login --token CLIENT_ID:CLIENT_SECRET:PID` (the three
   parts may also be separated by `|` or spaces). ATP checks them by
   looking the pid up. Without an H5 login, `whoami` shows the app with
   `extra.pid` and whether the pid was found (`extra.pid_found`).

`PDD_CLIENT_ID` and `PDD_CLIENT_SECRET` (with `PDD_PID`) in the
environment do the same without saving anything, and win over saved
credentials.

This route gives `search`, `read` (by `goods_sign`, or a goods id looked
up by search), `hot --category sales|realtime`, `categories [PARENT]` (the
goods tags) and `category ID`. It has no reviews and no shops. Errors name
the cause: a wrong secret is `signature_error`, an unknown app
`not_authenticated`, a pid that is not authorized `permission_denied`.

### Your login on the H5 site

- **Browser**: log in on https://mobile.yangkeduo.com (phone number and
  SMS code), then run `atp pdd login --browser`.
- **Cookie string**: `atp pdd login --cookie '...'` takes the `Cookie`
  header of a request to mobile.yangkeduo.com. `PDDAccessToken` and
  `pdd_user_id` are required; bring `api_uid`, `_nano_fp` and `pdd_vds` as
  the browser has them.
- `ATP_PDD_COOKIE` supplies such a cookie header for one run; it is never
  saved.

This route gives `read` with every SKU, the gallery, videos and the shop,
`comments`, `review-summary`, `user SHOP`, `user-posts SHOP`, `download`
of review pictures, and `search` when there are no 多多进宝 credentials.

How long an H5 session lasts is not known. When PDD no longer accepts it,
commands fail with `not_authenticated` ("the Pinduoduo session has
expired", or a page sent to the login) or, while PDD calls itself busy
(`系统繁忙`, code 40001), `rate_limited` with a hint that the session may
have expired.

### Without logging in

Visitors get the home page: `feed` (its recommendations, a few pages),
`hot` (the search box's hot words), `categories` (its tabs) and
`category ID` (a tab's goods). A share link's image (`_oak_share_url`)
downloads without a login too.

### Risk control

A risk check (code 54001) is `verification_required` with the check page
(`psnl_verification.html`) in `error.url`; on a terminal ATP opens it in a
browser (see `atp guide login`). Pass it there, or open
mobile.yangkeduo.com in a browser, then try again. PDD throttles the H5
site (`rate_limited`), the search most of all. Keep requests sparse: ATP
waits two seconds between calls. ATP does not solve PDD's captchas.

## References

- Goods: a goods id (`729007467138`),
  `https://mobile.yangkeduo.com/goods.html?goods_id=<id>` (also
  `goods1.html`, `goods2.html` and `mobile.pinduoduo.com`), share links
  (`goods.html?ps=…`), which are followed, a 多多进宝 `goods_sign`
  (`c9r2omogKFFAc7WBwvbZU1ikIb16_J3CTa8HNN`), the id of goods found through
  多多进宝, or `#N` after a list.
- Shops: a mall id or `https://mobile.yangkeduo.com/mall_page.html?mall_id=<id>`
  (the author `url` of a goods).
- Reviews: `GOODS/REVIEW` (goods id and `review_id`) or a review share
  link with `goods_id` and `review_id`, for `download`.
- Categories: the ids `categories` lists, or `#N` after it.

## Commands

Goods are posts of `kind` `item`. Prices are the group-buy price PDD shows
(`price`; `original` is the crossed-out price, `label` `券后` the price
after a coupon); the single-purchase price is `extra.single_price`.

### search

多多进宝 when configured, else the H5 search, which needs a login and which
PDD throttles hard (often one query per session before `rate_limited`).

### hot

`hot` (or `--category words`): the search box's hot words from the home
page. `--category sales` (今日销量榜) and `--category realtime` (实时热销榜)
are 多多进宝 rankings and need its credentials.

### feed

The home page's recommendations; visitors get a few pages.

### read

With an H5 login, the goods page: every SKU with its price and stock in
`extra.skus`, the gallery, videos and the shop. Otherwise 多多进宝 reads a
`goods_sign`, or a goods id it finds by search.

### download

`download GOODS` saves the gallery and videos. `download GOODS/REVIEW` (or
a review share link) saves one review's pictures and video; it needs a
login and looks through the first five review pages. A share link that
carries an image (`_oak_share_url`) downloads without a login.

### comments

20 reviews a page with stars, variant, pictures and video in `extra`.
Needs an H5 login and a goods id or link (not a `goods_sign`). PDD shows
only part of the reviews of popular goods.

### review-summary

A goods' rating and review tags with their counts. Needs an H5 login.

### user, user-posts

`user SHOP`: the shop card the goods page shows (with a login), else the
shop's page (shops may show their data only after a login). `user-posts SHOP`:
the shop's goods, best sellers first; needs a login.

### categories, category

With 多多进宝 credentials, `categories [PARENT]` lists the goods tags (a
tree: pass a tag id for the level below) and `category ID` searches a tag.
Without them, `categories` lists the home page's tabs (one level) and
`category ID` a tab's goods.

## Signing

The H5 calls carry `anti_content`, which ATP builds itself from a pure
port of the page's generator (chunk `3636`), presenting Chrome on an
Android phone. PDD changes that chunk with its releases; a stale port
shows up as `signature_error` or empty answers. 多多进宝 calls are signed
with the app secret instead.

## Limits

No QR login (the web login is phone and SMS only), no orders, carts,
coupons, group buying or merchant pages, and nothing written.
