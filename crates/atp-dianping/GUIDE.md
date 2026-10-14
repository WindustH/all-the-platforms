## Logging in

- **QR code**: `atp dp login` (or `--qrcode`) shows a code; scan it with
  the Dianping app and confirm there. The login is done once Dianping
  sets the cookie `dper`.
- **Browser**: log in on www.dianping.com, then run `atp dp login
  --browser`; it reads the cookies of dianping.com.
- **Cookie string**: copy the `Cookie` header of a request to
  www.dianping.com and run `atp dp login --cookie '...'`. It needs `dper`;
  include `_lxsdk_cuid`, `_hc.v` and `WEBDFPID` as the browser has them.
  `ATP_DIANPING_COOKIE` supplies such a header for one run and is never
  saved.

A login gives `search`, `comments`, `user` and `whoami`. Visitors get
`read` and `download` of a shop by its numeric id, notes, `feed`,
`cities` and `locate`. Visitors see phone numbers and addresses masked,
as Dianping shows them.

How long a login lasts is not known. When it ends, or the cookie `dper`
is missing, commands that need it fail with `not_authenticated` and the
hint to log in again.

### Risk control

Dianping bans busy accounts from review pages for hours or days, so ATP
waits three seconds between requests and stops at the first refusal.
When the gateway wants a human check (Yoda) or refuses a request (HTTP
403), ATP ends with `verification_required` and the page to open; on a
terminal it opens it in a browser, you pass the check there, and ATP
retries with that browser's cookies (see `atp guide login`). Account
risk (`101144`, `user_has_risk`) gives `permission_denied` and ATP does
not retry: use the app for a while or wait. Throttling (`200100`,
`频繁`, HTTP 429) gives `rate_limited`.

## References

- Shops: the numeric id (`563754`), the uuid (`H4KBo5JMg48nZcSa`), or
  `https://m.dianping.com/shop/<id>` / `https://www.dianping.com/shop/<uuid>`.
  The mobile page opens to visitors by numeric id; ATP remembers the uuid
  it names, which the review list wants.
- Notes: `https://m.dianping.com/discovery/<id>` (the `url` of a `feed`
  item) or `www.dianping.com/note/<id>`.
- Members: the numeric id or `https://www.dianping.com/member/<id>`.

## Commands

Shops are posts of kind `shop` with the spend per person in `price`;
notes are posts of kind `note`; reviews are comments; members are
users.

### read

`read SHOP`: name, category, region, spend per person (`price`, label
`¥90/人`), stars and sub-scores, hours, the ranking it is on (title, key
and page in `extra.rank`), deal and photo counts, and featured photos
and videos. Visitors see the phone number and address masked. `read
NOTE` takes a discovery note.

### download

`download SHOP` saves the shop's featured photos and videos.

### search

`search WORDS` (`--filter food|fun|beauty|…` for a channel): 15 shops a
page with stars, reviews, spend and address. It runs in `DIANPING_CITY`
(a Dianping city id from `cities` or `locate`), else your session's city
(cookie `cy`), else Shanghai. Needs a login; when Dianping shows a check
instead of results, ATP ends with `verification_required`.

### comments

`comments SHOP`: 14 reviews a page (author, text, time, stars,
sub-scores, spend per person, pictures); merchant replies are left out.
Needs a login. A numeric shop id is turned into the shop's uuid through
its mobile page once.

### feed

`feed [--type food|shopping|…]`: the discovery page's notes, 80 a page,
open to visitors.

### user

`user MEMBER`: name, city and counts from the member's page. Needs a
login.

### cities, locate

`cities [NAME]`: Dianping's ids of the larger cities. `locate LAT LNG`:
the city at a point with its Dianping id (any city), through Meituan's
lookup. Both are open to visitors.

## Signing

Requests go out as the site's own pages send them: your cookies, the
page as `Referer`, the H5 flags and the rohr `_token`. Dianping's JSON
routes also want `mtgsig`, a signature from Meituan's H5guard script;
ATP does not make or imitate it. If a route keeps refusing after you
pass the check, it wants `mtgsig`.

## Limits

No rankings (their pages fetch the list encrypted), hot words or
favorites; no ordering, deals purchase, coupons or bookings; nothing is
written (no reviews, notes, likes or favorites). ATP does not solve
Dianping's captchas.
