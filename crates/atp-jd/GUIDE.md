## References

- Items: SKUs (`100012043978`), `https://item.jd.com/<sku>.html`,
  `item.m.jd.com/product/<sku>.html`, `item.jd.hk/<sku>.html`,
  `npcitem.jd.hk/<sku>.html`, links with `sku=` / `skuId=` / `wareId=`,
  and `3.cn` / `u.jd.com` short links.
- Shops: `https://mall.jd.com/index-<id>.html` or a bare shop id (the
  author `url` of an item).

## Reading

- `hot`: the search box's hot words (mostly promotions, a few at a time).
- `suggest KEYWORD` and `related ITEM`: words the search box suggests and
  searches an item page links to. These need no login and no signature.
- `search` (`--sort relevance|sales|price-asc|price-desc|comments`), 30
  items a call, with the price shown (`price`; `label` names a coupon
  price such as 到手价), shop, sales and rating snippets (`tags`). Prices
  and stock follow the delivery area of the session (`ipLoc-djd`,
  Beijing by default).
- `read ITEM` shows title, price (with the crossed-out one), shop, stock,
  brand, category, attributes and images; `download ITEM` saves the
  images.
- `comments ITEM`: the reviews on the item page, then further pages of
  the review list (score, images, variant bought in `extra`); the first
  page notes the review count and good rate.
- `rating ITEM`: the item's review summary: review count, good rate and
  the review tags with their counts.
- `user SHOP`: the shop's name, vender id and main category.
- `user-posts SHOP`: the shop's items, 20 a page, as the mobile shop
  site's "all items" tab lists them.

## Signing and risk control

Search, item pages, reviews and shop items are signed with h5st 5.3,
ported from JD's `js_security_v3` without running it: atp keeps one
fingerprint per signing app id and fetches its token from
`cactus.jd.com/request_algo` (cached for a day). JD checks the signature
at its edge (a wrong one gets an empty 403) and then scores the client: a
client without JD's device cookies and login is usually sent to a risk
check (code 605), which surfaces as `verification_required` with the
check page in `url`. Item details (`read`) are the best guarded and
refuse new clients with empty 403s (`rate_limited`). A logged-in browser
session (`login --browser`, which brings the device cookies
`3AB9D23F7A4B3CSS`, `shshshfp*` and `__jda` along) is what JD expects;
whether it gets past the check depends on that browser's standing. JD
bans clients that call fast: keep reads sparse.

## Logging in

Log in on www.jd.com in a browser and run `atp jd login --browser`, or pass
the cookies with `--cookie` (`thor`, `pin`, plus `ipLoc-djd`,
`3AB9D23F7A4B3CSS`, `__jda`, `__jdu` and `shshshfp*` as the browser has
them). With a login, `favorites` (followed items) and `history` (browsed
items) list your own items.

## Limits

No QR login, no customer-service chats, and nothing transactional (cart,
orders, coupons). atp does not solve JD's captchas.
