ATP reaches Meituan by two routes: the official 美团联盟 (CPS) open API
for deals and orders, and your Dianping login for shop reviews. City and
area lookups are open to visitors.

## Logging in

The two routes have separate logins; one session can hold both. `whoami`
shows the 美团联盟 app when its key is set, else the Dianping account.

### 美团联盟 key and secret (deals and orders)

1. Open your 美团联盟 media account at media.meituan.com; the key and
   secret are under 我的 → API 接口.
2. Run `atp mt login --token 'APP_KEY:APP_SECRET'` (`--token -` reads
   it from stdin). ATP checks the pair with a one-deal query and keeps it
   in the session.

`MEITUAN_APP_KEY` and `MEITUAN_APP_SECRET` in the environment do the
same without saving anything, and win over a saved pair.

This route gives `search`, `hot`, `read`, `deals`, `download` and
`insights`. Requests are signed with your secret (HMAC-SHA256, as
Meituan's SDKs do) and carry a timestamp that is valid for two minutes:

- a wrong secret gives `signature_error` with a hint to check it;
- a wrong clock on this machine gives `signature_error` with a hint to
  check the clock;
- a key 美团联盟 refuses (HTTP 401 or 403, or an authorization message)
  gives `not_authenticated` with a hint to log in with `--token` again;
- an API your media account has not opened (`权限`, `未开通`) gives
  `permission_denied`; throttling (`限流`, `qps`) gives `rate_limited`.

### Dianping login (shop reviews)

Meituan's 到店 shops show Dianping's reviews, read from Dianping's
backend, so `comments` uses a Dianping login:

1. Log in on www.dianping.com in your browser.
2. Run `atp mt login --browser`. It reads the cookies of meituan.com and
   dianping.com. `atp mt login` with no flag does the same.

Or copy the `Cookie` header of a request to www.dianping.com and run
`atp mt login --cookie '...'`. The review list needs the cookie `dper`;
without it `comments` fails with `not_authenticated`.
`ATP_MEITUAN_COOKIE` supplies a cookie header for one run and is never
saved.

### Without logging in

`locate` and `areas` work for visitors. Everything else needs one of the
two routes above.

### How long a session lasts

Not known for either route. When the Dianping login ends, `comments`
fails with `not_authenticated` ("Dianping wants a login for this"); when
美团联盟 stops accepting the key, the API commands fail with
`not_authenticated` and the hint to run `login --token` again.

### Checks and account risk

When Meituan's or Dianping's gateway wants a human check (Yoda, on
verify.meituan.com) or refuses a request with HTTP 403 or 418, ATP ends
with `verification_required` and the page to open. On a terminal it opens
that page in a browser; you pass the check there and ATP retries with
that browser's cookies (see `atp guide login`). Keep requests sparse.

- Account risk (`101144`, `user_has_risk`) gives `permission_denied`:
  ATP stops instead of retrying. Use the app for a while or wait before
  trying again.
- Throttling (`200100`, `频繁`, HTTP 429) gives `rate_limited`: wait a
  few minutes.
- HTTP 460 means the route is refused to web clients
  (`permission_denied`).
- ATP waits two seconds between requests.

## References

- Deals: the id `search`, `hot` or `deals` shows (a `productViewSign`,
  or an older `skuViewId`), or `#N` after a list.
- Shops for `comments`: the shop as Dianping names it: its numeric id,
  its uuid or a `dianping.com/shop/…` link (see `atp guide dianping`).
- Cities and areas: Meituan's ids from `locate LAT LNG` (Shanghai is 10)
  and `areas CITY`.

## Commands

Deals are posts of kind `deal`: the selling price in `price`
(`original` is the crossed-out one), the shop or brand as author, sales
in `metrics.sold`, Meituan's labels (`点评高分4.6` …) in `tags`, and
commission, available shops, validity and delivery details in `extra`.
Without a location Meituan answers for Beijing.

### search

`search WORDS` covers every category; `--sort` is `relevance` or
`price` (low to high). For a city, an area or a point use `deals WORDS`.

### hot

`--category today|local|realtime`: the 到店 rankings 今日必推, 同城热销
and 实时热销, 20 deals each, without further pages. `deals --rank
picks|trending` gives the 到家 ones (精选, 跟推爆款).

### read

Takes a deal id from a list. ATP asks by `productViewSign` first, then
by the older `skuViewId`; a deal no longer on sale is `not_found`.

### download

Saves a deal's picture; it needs the 美团联盟 key, like `read`.

### comments

`comments SHOP`: 14 reviews a page from Dianping's review list, with
stars, sub-scores, spend per person and pictures in `extra`. Merchant
replies (商家回应) are left out. Needs the Dianping login.

### insights

`insights [--days N]`: your media account's orders paid in the last N
days (at most 90): counts, amounts, estimated commission, refunds, a
breakdown by status and category, and a daily series (Beijing days).
商品超值券包 and 省钱包 orders are not included. ATP reads at most 2000
orders and warns when it stops there; a shorter `--days` covers the
rest. `insights POST` is not supported.

### deals

`deals [WORDS]` lists deals with every filter the API takes: `--rank`,
`--store` (到店 instead of 到家 delivery), `--city`, `--area`, `--near
LAT,LNG` and `--sort price|sales|commission`.

- `--rank` wins over WORDS.
- With WORDS the search covers every category, so `--store` has no
  effect and only `--sort price` applies.
- Rankings (`--rank`) hold 20 deals without further pages; `--sort
  price` does not apply to 到店 rankings.
- Plain lists (no WORDS, no `--rank`) ignore `--sort commission`.

### locate

`locate LAT LNG`: the city with Meituan's and Dianping's ids, the
district and the business area at a point. `locate --ip [IP]`: where an
IP address is (yours without one; Meituan places mainland addresses
only). Open to visitors.

### areas

`areas CITY`: the city's districts; `--district ID|NAME` lists that
district's business areas, whose ids `deals --area` takes. Open to
visitors.

## Limits

- The 美团联盟 calls are signed with your own secret, as Meituan's SDKs
  do. Meituan's own web pages and APIs want `mtgsig`, a signature its
  H5guard script makes; ATP does not make or imitate it, which is why
  shops, menus and delivery searches are not here.
- No ordering, payment, coupons, cart or refunds; no referral links; no
  merchant back-office; no QR login (Meituan's web login is SMS or
  password). Nothing is written.
