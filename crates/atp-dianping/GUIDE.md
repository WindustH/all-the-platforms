## Logging in

- `atp dp login`: scan the QR code with the Dianping app and confirm.
- `atp dp login --browser` after logging in on www.dianping.com, or
  `--cookie` with `dper` (plus `_lxsdk_cuid`, `_hc.v`, `WEBDFPID` as the
  browser has them).

Visitors get `read SHOP` (by numeric id), notes and `feed`; search,
reviews and member pages want a login.

## References

- Shops: the numeric id (`563754`), the uuid (`H4KBo5JMg48nZcSa`), or
  `https://m.dianping.com/shop/<id>` / `https://www.dianping.com/shop/<uuid>`.
  The mobile page opens to visitors by numeric id; atp remembers the uuid
  it names, which the review list wants.
- Notes: `https://m.dianping.com/discovery/<id>` (the `url` of a `feed`
  item) or `www.dianping.com/note/<id>`.
- Members: the numeric id or `https://www.dianping.com/member/<id>`.

## Reading

- `read SHOP`: name, category, region, spend per person (`price`, label
  `¥90/人`), stars and sub-scores, hours, the ranking it is on (title,
  key and page in `extra.rank`), deal and photo counts, and featured
  photos and videos. Visitors see the phone number and address masked,
  as Dianping shows them. `download SHOP` saves the photos and videos.
- `search WORDS` (`--filter food|fun|beauty|…` for a channel): 15 shops
  a page with stars, reviews, spend and address. It runs in
  `DIANPING_CITY` (an id from `cities` or `locate`), else your session's
  city (cookie `cy`), else Shanghai.
- `comments SHOP`: 14 reviews a page (author, text, time, stars,
  sub-scores, pictures); merchant replies are left out.
- `feed [--type food|shopping|…]`: the discovery page's notes, 80 a page.
- `user MEMBER`: name, city and counts from the member's page.
- `cities [NAME]`: Dianping's ids of the larger cities; `locate LAT LNG`:
  the city at a point with its Dianping id.

## Signing and risk control

Requests go out as the site's own pages send them: your cookies, the
page as `Referer`, the H5 flags and the rohr `_token`. Dianping's JSON
routes also want `mtgsig`, a signature from Meituan's H5guard script;
atp does not make or imitate it. When the gateway wants a human check
(Yoda) or refuses a request (HTTP 403), atp ends with
`verification_required` and the page to open; on a terminal it opens it
in your Chrome, you pass the check there, and atp retries with that
browser's cookies. If a route keeps refusing, it wants `mtgsig`.
Dianping bans busy accounts from review pages for hours or days, so atp
waits three seconds between requests and stops at the first refusal.

## Limits

No rankings (their pages fetch the list encrypted), hot words or
favorites; no ordering, deals purchase, coupons or bookings; nothing is
written (no reviews, notes, likes or favorites). atp does not solve
Dianping's captchas.
