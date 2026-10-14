## Logging in

- **QR code**: `atp taobao login` shows the QR code of Taobao's web login
  (login.taobao.com). Scan it with the Taobao (淘宝) app and confirm there.
  A code that expires or is cancelled ends the login; run it again.
- **Face or SMS check**: Taobao may want to confirm a new login first. The
  QR login then fails with `verification_required` and the page to do the
  check on (`error.url`): finish the check there in a browser, then run
  `atp taobao login --browser`. When the scan is confirmed but no session
  arrives, log in on www.taobao.com in a browser and import that session.
- **Browser**: log in on https://www.taobao.com, then
  `atp taobao login --browser` reads that browser's taobao.com and
  tmall.com cookies.
- **Cookie string**: `atp taobao login --cookie '...'` takes the `Cookie`
  header of a request to www.taobao.com (developer tools, Network tab).
  `unb` (the user id) and `cookie2` are required; bring `_nk_` (the
  nickname chat messages carry), `cna`, `isg` and `x5sec` as the browser
  has them. `_m_h5_tk`, the token every call is signed with, may come
  along; ATP fetches a fresh one when it is missing or expired.
- `ATP_TAOBAO_COOKIE` supplies such a cookie header for one run; it is
  never saved.

Taobao serves nothing to visitors: everything but `suggest` needs a login.
`login` and `whoami` check a session by getting a chat access token, which
Taobao issues to working sessions only.

How long a session lasts is not known. When it ends, commands fail with
`not_authenticated` ("Taobao wants a login", or "Taobao shows item pages to
logged-in users only") and a hint naming the ways to log in. ATP gets a new
chat access token every 6 hours by itself.

### Risk control

- Taobao's risk control watches direct API calls closely. When it answers
  `RGV587` ("被挤爆啦") or asks for a slider, that is
  `verification_required` with the page in `error.url`; on a terminal ATP
  opens it in a browser (see `atp guide login`). Otherwise open the page
  in your browser, pass the check, and import the cookies again
  (`login --browser`); retrying at once does not help.
- Item pages (`user`, `send item:<id>`) meet a slider of their own: open
  the item link in a browser, pass the check, then run
  `atp taobao login --browser`.
- Traffic limits (`FAIL_SYS_TRAFFIC_LIMIT`, `FLOWLIMIT`) are
  `rate_limited`. ATP waits 2 seconds between requests.

## References

- Items: `https://item.taobao.com/item.htm?id=<id>`,
  `https://detail.tmall.com/item.htm?id=<id>` (and their mobile forms),
  `m.tb.cn/…` share links, share text with such a link in it, a bare item
  id (6 digits or more), or `#N` after a list.
- Sellers: Taobao has no public user pages; `user ITEM` shows the seller
  of an item.
- Chats: the ids `chats` lists (`<uid>.1-<uid>.1#11001`, `@cntaobao` may
  follow), or `#N` after `chats`. `send` also takes `item:<id>`, an item id
  or an item link: it writes to that item's shop.

## Commands

Items are posts of `kind` `item` with their price in `price` (yuan, as
shown) and the shop as the author.

### search

48 items a page, each with its sales (`extra.sales`), location
(`extra.location`) and `extra.tmall` for Tmall shops.

### suggest

The words the search box suggests for a keyword. Works without a login.

### read, download

`read ITEM` shows the title, images, price, sales (`extra.sales`), shop
(`extra.shop_id`) and how many variants it has (`extra.skus`).
`download ITEM` saves the images.

### comments

An item's reviews, 20 a page; `--sort pictures|good|neutral|bad` picks
those with pictures or of one grade. Nicknames come masked. `extra.sku` is
the variant bought, `extra.images` the review's pictures, `extra.appended`
the follow-up review.

### user

`user ITEM` shows the seller of an item, read from the item page.

### chats

Your chats with shops, the most recently active first; each names the
item it is about (`description`, `extra.item`) and the other side
(`extra.peer`).

### messages

Pages back through a chat, 20 messages at a time. An item is not a chat:
find the chat with its shop in `chats`.

### send

`send CHAT TEXT` writes to a chat; `-a FILE` adds images (images only,
each sent as a message of its own). `send item:<id> TEXT` writes to the
item's shop, opening the chat; writing to your own shop's item is refused.
`--reply-to` is not supported.

### listen

Prints messages shops send as they arrive, from every chat or only the
chat ids given (not items). The connection is opened again when Taobao
drops it.

## Limits

No orders, carts, coupons or reviews of your own.
