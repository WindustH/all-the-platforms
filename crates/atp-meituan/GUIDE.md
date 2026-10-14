## Ways in

- **美团联盟 (official CPS open API)**: `atp mt login --token
  APP_KEY:APP_SECRET`, or `MEITUAN_APP_KEY` and `MEITUAN_APP_SECRET` in
  the environment (they win). The key and secret belong to a 美团联盟
  media account (media.meituan.com, 我的 → API 接口); requests are signed
  with them (HMAC-SHA256), so a wrong clock or secret gives
  `signature_error`. This gives `search`, `hot`, `read`, `deals`,
  `download` and `insights`; `whoami` checks the credentials.
- **Your Dianping login**, for reviews: log in on www.dianping.com in your
  browser and run `atp mt login --browser` (it reads the cookies of
  meituan.com and dianping.com). Meituan's 到店 shops show Dianping's
  reviews, read from Dianping's backend, so `comments` uses that login.
- **Visitors** get `locate` and `areas`.

## References

- Deals: the id `search`, `hot` or `deals` shows (a `productViewSign`,
  or an older `skuViewId`), or `#N` after a list.
- Shops for `comments`: the shop as Dianping names it: its numeric id,
  its uuid or a `dianping.com/shop/…` link (see `atp guide dianping`).
- Cities and areas: Meituan's ids from `locate LAT LNG` (Shanghai is 10)
  and `areas CITY`.

## Reading

- `search WORDS` (`--sort relevance|price`) covers every category;
  `deals [WORDS]` adds `--rank`, `--store` (到店 instead of 到家
  delivery), `--city`, `--area`, `--near LAT,LNG` and `--sort
  price|sales|commission`. Without a location Meituan answers for
  Beijing.
- `hot --category today|local|realtime`: the 到店 rankings 今日必推,
  同城热销 and 实时热销, 20 deals each. `deals --rank picks|trending`
  gives the 到家 ones (精选, 跟推爆款).
- Deals show the selling price in `price` (`original` is the crossed-out
  one), the shop or brand as author, sales in `metrics.sold`, Meituan's
  labels (`点评高分4.6` …) in `tags`, and commission, available shops,
  validity and delivery details in `extra`.
- `insights [--days N]`: your media account's orders paid in the last N
  days (at most 90): counts, amounts, estimated commission, refunds, a
  breakdown by status and category, and a daily series. 商品超值券包 and
  省钱包 orders are not included.
- `comments SHOP`: 14 reviews a page from Dianping's review list, with
  stars, sub-scores and pictures in `extra`.

## Lookups

- `locate LAT LNG`: the city with Meituan's and Dianping's ids, the
  district and the business area at a point. `locate --ip [IP]`: where
  an IP address is (Meituan places mainland addresses only).
- `areas CITY`: the city's districts; `--district ID|NAME` lists that
  district's business areas, whose ids `deals --area` takes.

## Signing and risk control

The 美团联盟 calls are signed with your own secret, as Meituan's SDKs do.
Meituan's own web pages and APIs want `mtgsig`, a signature its H5guard
script makes; atp does not make or imitate it, which is why shops,
menus and delivery searches are not here. When Meituan's or Dianping's
gateway wants a human check (Yoda) or refuses a request, atp ends with
`verification_required` and the page to open; on a terminal it opens it
in your Chrome, you pass the check there, and atp retries with that
browser's cookies. Account risk (`101144`) gives `permission_denied`:
atp stops instead of retrying. atp waits two seconds between requests.

## Limits

No ordering, payment, coupons, cart or refunds; no referral links; no
merchant back-office; no QR login (Meituan's web login is SMS or
password). Nothing is written.
