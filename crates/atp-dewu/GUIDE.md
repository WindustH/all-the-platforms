## Logging in

Dewu has no web login ATP could use: there are no account sessions and no
cookies. Everything but the official market data works without
credentials, and nothing is written. Where the site wants a user login,
the call fails with `not_authenticated`.

### POIZON Open Platform app

The POIZON Open Platform gives article-number lookups, per-size prices
and 30-day sales (`search`, `prices`, `sales`).

1. Create an app on https://open.poizon.com.
2. Put this machine's public IP on the app's IP whitelist (应用详情 →
   IP白名单); calls from other addresses are refused.
3. Run `atp dewu login --token APP_KEY:APP_SECRET`, the app's key and
   secret (they may also be separated by `|` or a space). ATP checks them
   with one catalogue lookup, as `whoami` does.

`DEWU_APP_KEY` and `DEWU_APP_SECRET` in the environment supply them
without saving anything, and win over saved ones.

The calls need only the app's signature, no OAuth token, so nothing
expires that ATP knows of. Failures name their cause: an address off the
whitelist is `permission_denied` (code 5025), a wrong secret
`signature_error`, an unknown key `not_authenticated`, an API the app has
no permission package for `permission_denied` (apply for it in the
console), too many calls `rate_limited`.

Merchant keys of the mainland platform (openapi.dewu.com) are not
supported: it serves a merchant's own bids and orders, no market prices.

### Risk control

Dewu's user agreement forbids crawlers and automated tools; keep to the
odd lookup by hand. ATP waits three seconds between requests and never
pages further than `--limit`. A captcha demand on the site (code 485) is
`verification_required` with the page in `error.url`. ATP does not open it
in a browser: its requests carry no cookies and lack the site's device
headers, so a check passed there would not clear it. See "Signing and
refusals" below for which calls meet it.

## References

- Products: a `spuId` (`1234567`, or `spu:1234567`), or a link with
  `spuId=`: `https://www.dewu.com/product-detail.html?spuId=<id>`,
  `m.dewu.com` / `cdn-m.dewu.com` `router/product/ProductDetail`,
  `fast.dewu.com` / `cdn-fast.dewu.com` `page/productDetail`.
- Community posts: `trend:<id>` or a share link
  (`https://m.dewu.com/rn-activity/community-share?trendId=<id>`). A bare
  number is read as a product.
- Short links (`https://dw4.co/t/A/…`) and `fast.dewu.com` /
  `cdn-fast.dewu.com` links without an id are followed to the page they
  open.
- Article numbers (货号) such as `DD1391-100` for `search`, `prices` and
  `sales`.
- Home page tabs: the ids `tabs` lists, or `#N` after it.

## Commands

Products are posts of `kind` `item` with `spuId`s as ids; community posts
are posts of kind `video` or `post`.

### search

`search ARTICLE` (Open Platform): the products filed under an article
number (`DD1391-100`), with their sizes in `extra.sizes`. A lowercase
article number that finds nothing is tried in capitals.

### prices

`prices ARTICLE [--region CN] [--currency CNY] [--consign]` (Open
Platform): each size's lowest prices (global, Asia, local, US, 95分
reference, other platforms, cross-region); `price` is the lowest global
one. Spot (现货) bids by default, consignment (寄售) with `--consign`.
Zero-decimal currencies (JPY, KRW) come back wrong from the API.

### sales

`sales ARTICLE` (Open Platform): each size's sales over the last 30 days
and their change on the month before.

### hot, tabs, tab

`hot`: products of the first tab of the www.dewu.com home page, 24 a
page, with the price the page shows. `tabs` lists the tabs and `tab ID` a
tab's products. Dewu often refuses the products (see "Signing and
refusals").

### feed

The hot list of the community page (www.dewu.com/community), 32 posts a
page, with cover, author and likes.

### read, download

- `read PRODUCT`: title, brand, article number (货号), retail price,
  release date, parameters and gallery. `price` is the lowest buy price
  over all sizes; `extra.sizes` lists every size with its SKU, retail
  price and buy price. When Dewu refuses the prices, the page's own price
  stays and `extra.buy_prices_error` says why.
- `read POST`: a community post's title, text, topics, author name and its
  video or images.
- `download POST` saves the video without watermark (720p) or the images,
  `download PRODUCT` the gallery.

## Signing and refusals

The PC calls carry the site's md5 `sign`; ATP does not make the device
headers (`SK`, `ltk`, `shumeiId`) the site's protection script adds. The
tabs and the community hot list answer without them; a tab's products
(`hot`, `tab`) answer with a captcha demand (code 485,
`verification_required`) and product pages (`read`, `download` of a
product) with a refusal (`请求已拒绝`, code 7999, `upstream_error`), both
with the page to open in `url`. Share pages and the Open Platform need
neither.

## Limits

Keyword search, recent sales, comments, user profiles, follows, likes and
chats exist only in the app and are not supported, nor are logins, orders,
bids or anything behind a captcha.
