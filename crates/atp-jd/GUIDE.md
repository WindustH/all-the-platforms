## Logging in

JD has no QR login here, so `atp jd login` imports the session from a
browser.

- **Browser**: log in on https://www.jd.com, then `atp jd login --browser`
  reads that browser's jd.com cookies. They include the device cookies
  JD's pages set (`3AB9D23F7A4B3CSS`, `shshshfp*`, `__jda`), which ATP
  sends along and which JD's risk control expects.
- **Cookie string**: `atp jd login --cookie '...'` takes the `Cookie`
  header of a request to www.jd.com (developer tools, Network tab). `thor`
  and `pin` are required (the mobile site's `pt_key` and `pt_pin` are
  accepted by the calls as well). Bring `ipLoc-djd` (the delivery area),
  `3AB9D23F7A4B3CSS`, `__jda`, `__jdu` and `shshshfp*` as the browser has
  them.
- `ATP_JD_COOKIE` supplies such a cookie header for one run; it is never
  saved.

A login lists your own items: `favorites` (followed items) and `history`
(browsed items). `hot`, `suggest` and `related` need neither a login nor a
signature. `search`, `read`, `comments`, `rating`, `user` and `user-posts`
do not require a login either, but JD usually sends a client without its
device cookies and a login to a risk check (see "Signing and risk control"
below).

How long a session lasts is not known. `whoami` asks passport.jd.com
whether the session is still logged in; when it is not, commands fail
with `not_authenticated` ("the JD session has expired", or "JD wants a
login") and a hint naming the ways to log in.

JD bans clients that call fast: keep reads sparse. ATP waits 3 seconds
between requests.

## References

- Items: SKUs (`100012043978`), `https://item.jd.com/<sku>.html`,
  `item.m.jd.com/product/<sku>.html`, `item.jd.hk/<sku>.html`,
  `npcitem.jd.hk/<sku>.html`, links with `sku=` / `skuId=` / `wareId=`,
  `3.cn` / `u.jd.com` short links (followed), or `#N` after a list.
- Shops: `https://mall.jd.com/index-<id>.html` (or `shop.jd.com`), links
  with `shopId=`, or a bare shop id (the author `url` of an item).

## Commands

Items are posts of `kind` `item` with SKUs as ids and the price in
`price`. Prices and stock follow the delivery area of the session
(`ipLoc-djd`, Beijing by default).

### search

30 items a call, with the price shown (`price`; `label` names a coupon
price such as 到手价), the shop, sales and rating snippets (`tags`).
`--sort comments` puts the most reviewed first.

### hot

The search box's hot words (mostly promotions), a few at a time. No login
or signature needed.

### suggest, related

`suggest KEYWORD`: the words the search box suggests. `related ITEM`: the
searches an item page links to. No login or signature needed.

### read, download

`read ITEM` shows the title, price (with the crossed-out one), shop
(`extra.vender_id`), stock (`extra.stock`), brand, category, attributes,
variants and images. `download ITEM` saves the images. Item details are
JD's best-guarded call (see "Signing and risk control").

### comments

The reviews on the item page, then further pages of the review list. The
first page notes the review count and good rate. `extra` holds the score,
the pictures (`images`) and the variant bought.

### rating

The item's review summary: review count, good rate and the review tags
with their counts.

### user, user-posts

`user SHOP`: the shop's name, vender id and main category. `user-posts
SHOP`: the shop's items, 20 a page, as the mobile shop site's "all items"
tab lists them.

### favorites, history

Your followed items and your browsing history, 20 a page. Both need a
login.

## Signing and risk control

Search, item pages, reviews, shop items, favorites and history are signed
with h5st 5.3, ported from JD's `js_security_v3` without running it: ATP
keeps one fingerprint per signing app id and fetches its token from
`cactus.jd.com/request_algo` (cached for a day). When that fails ATP signs
with a local token, which JD's business APIs refuse; the error then says
so.

JD checks the signature at its edge (a wrong one gets an empty 403) and
then scores the client. A client
without JD's device cookies and login is usually sent to a risk check
(code 605, or a redirect to JD's risk handler): `verification_required`
with the check page in `error.url`, which ATP opens in a browser on a
terminal (see `atp guide login`). Item details (`read`) are the best
guarded and refuse new clients with empty 403s (`rate_limited`). The hint
of both is the same: open www.jd.com in a browser, then run
`atp jd login --browser` so ATP uses that browser's identity, or try again
later. Whether a browser session gets past the check depends on that
browser's standing.

## Limits

No QR login, no customer-service chats, and nothing transactional (cart,
orders, coupons). ATP does not solve JD's captchas.
