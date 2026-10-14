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

## Reading

Everything works without an account, and nothing is written.

- `hot`: products of the first tab of the www.dewu.com home page, 24 a
  page, with the price the page shows. `tabs` lists the tabs and `tab ID`
  a tab's products.
- `feed`: the hot list of the community page (www.dewu.com/community), 32
  posts a page, with cover, author and likes.
- `read PRODUCT`: title, brand, article number (货号), retail price,
  release date, parameters and gallery. `price` is the lowest buy price
  over all sizes; `extra.sizes` lists every size with its SKU, retail
  price and buy price. When Dewu refuses the prices, the page's own price
  stays and `extra.buy_prices_error` says why.
- `read POST`: a community post's title, text, topics, author name and its
  video or images; `download POST` saves the video without watermark
  (720p) or the images, `download PRODUCT` the gallery.

## Official market data

The POIZON Open Platform gives article-number lookups, per-size prices
and 30-day sales. Create an app on https://open.poizon.com, put this
machine's public IP on its whitelist, then run `atp dewu login --token
APP_KEY:APP_SECRET` (or set `DEWU_APP_KEY` and `DEWU_APP_SECRET`, which
win). `whoami` checks the app with one lookup.

- `search ARTICLE`: the products filed under an article number
  (`DD1391-100`), with their sizes.
- `prices ARTICLE [--region CN] [--currency CNY] [--consign]`: each
  size's lowest prices (global, Asia, local, US, 95分 reference, other
  platforms, cross-region); `price` is the lowest global one. Spot (现货)
  bids by default, consignment (寄售) with `--consign`. Zero-decimal
  currencies (JPY, KRW) come back wrong from the API.
- `sales ARTICLE`: each size's sales over the last 30 days and their
  change on the month before.

Merchant keys of the mainland platform (openapi.dewu.com) are not
supported: it serves a merchant's own bids and orders, no market prices.

## Signing and refusals

The PC calls carry the site's md5 `sign`; atp does not make the device
headers (`SK`, `ltk`, `shumeiId`) the site's protection script adds. The
tabs and the community hot list answer without them; a tab's products
(`hot`, `tab`) answer with a captcha demand (code 485,
`verification_required`) and product pages (`read`, `download` of a
product) with a refusal (`请求已拒绝`, code 7999, `upstream_error`), both
with the page to open in `url`. Share pages and the Open Platform need
neither.

## Limits and terms

Dewu's user agreement forbids crawlers and automated tools; keep to the
odd lookup by hand. atp waits three seconds between requests and never
pages further than `--limit`. Keyword search, recent sales, comments,
user profiles, follows, likes and chats exist only in the app and are not
supported, nor are logins, orders, bids or anything behind a captcha.
