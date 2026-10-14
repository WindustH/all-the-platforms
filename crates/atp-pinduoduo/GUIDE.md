## References

- Goods: a goods id (`729007467138`), `https://mobile.yangkeduo.com/goods.html?goods_id=<id>`
  (also `goods1.html`, `goods2.html` and `mobile.pinduoduo.com`), share
  links (`goods.html?ps=…`), which are followed, or a 多多进宝 `goods_sign`
  (`c9r2omogKFFAc7WBwvbZU1ikIb16_J3CTa8HNN`), the id of goods found
  through 多多进宝.
- Shops: a mall id or `https://mobile.yangkeduo.com/mall_page.html?mall_id=<id>`
  (the author `url` of a goods).
- Reviews: `GOODS/REVIEW` (goods id and `review_id`) or a review share
  link with `goods_id` and `review_id`, for `download`.

## Two ways in

- **Visitors** get the home page: `feed` (its recommendations, a few
  pages), `hot` (the search box's hot words), `categories` (its tabs) and
  `category ID` (a tab's goods).
- **多多进宝 (official open API)**: `atp pdd login --token
  CLIENT_ID:CLIENT_SECRET:PID`, or `PDD_CLIENT_ID`, `PDD_CLIENT_SECRET` and
  `PDD_PID` in the environment (they win). Create an app on
  open.pinduoduo.com (多多进宝 type) and a 推广位 (pid) on
  jinbao.pinduoduo.com; PDD wants the pid authorized (备案) once before
  searches. This gives `search`, `read` (by `goods_sign`, or a goods id
  looked up by search), `hot --category sales|realtime` (今日销量榜,
  实时热销榜), `categories [PARENT]` (goods tags) and `category ID`.
  `whoami` lists the app's pid. No reviews and no shops this way.
- **Your login** on the H5 site: `atp pdd login --browser` after logging in
  on mobile.yangkeduo.com (phone number and SMS code), or `--cookie` with
  `PDDAccessToken` and `pdd_user_id` (plus `api_uid`, `_nano_fp`,
  `pdd_vds` as the browser has them). This gives `read` with every SKU,
  the gallery, videos and the shop, `comments`, `review-summary`, `user
  SHOP`, `user-posts SHOP` (its goods, best sellers first), `download` of
  review pictures, and `search` when there are no 多多进宝 credentials.

## Reading

- Prices are the group-buy price PDD shows (`price`; `original` is the
  crossed-out price, `label` `券后` for the price after a coupon); the
  single-purchase price is `extra.single_price`. `read` lists SKUs with
  their prices and stock in `extra.skus`.
- `search` (`--sort relevance|sales|price-asc|price-desc`): 多多进宝 when
  configured, else the H5 search, which PDD throttles hard (often one
  query per session before `rate_limited`).
- `comments GOODS`: 20 reviews a page with stars, variant, pictures and
  video in `extra`; PDD shows only part of the reviews of popular goods.
- `download GOODS` saves the gallery and videos; a share link's image
  (`_oak_share_url`) downloads without a login.

## Signing and risk control

The H5 calls carry `anti_content`, which atp builds itself from a pure
port of the page's generator (chunk `3636`), presenting Chrome on an
Android phone. PDD changes that chunk with its releases; a stale port
shows up as `signature_error` or empty answers. A risk check (`54001`)
surfaces as `verification_required` with the check page in `url`; a
session PDD no longer accepts gives `not_authenticated` or, while PDD
calls itself busy (`系统繁忙`, `40001`), `rate_limited`. Keep requests
sparse: atp waits two seconds between calls.

## Limits

No QR login (the web login is phone and SMS only), no orders, carts,
coupons, group buying or merchant pages, and nothing written. atp does not
solve PDD's captchas.
