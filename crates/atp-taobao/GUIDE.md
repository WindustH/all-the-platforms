## References

- Items: `https://item.taobao.com/item.htm?id=<id>`,
  `https://detail.tmall.com/item.htm?id=<id>` (and their mobile forms),
  `m.tb.cn/…` share links, share text with such a link in it, or a bare
  item id.
- Sellers: Taobao has no public user pages; `user ITEM` shows the seller
  of an item.
- Chats: the ids `chats` lists (`<uid>.1-<uid>.1#11001`). `send` also takes
  `item:<id>` or an item link: it opens the chat with that item's shop.

## Reading

- `search` (`--sort relevance|sales|price-asc|price-desc`), 44 items a
  page, with price (`extra.price`, yuan as shown), sales, location and
  shop.
- `read ITEM` shows the title, images, price, sales and shop;
  `download ITEM` saves the images.
- `comments ITEM` lists reviews, 20 a page
  (`--sort all|pictures|good|neutral|bad`); nicknames come masked.
- `suggest KEYWORD`: the words the search box suggests (no login needed).

## Chats

- `chats` lists your chats with shops; `messages CHAT` pages back through
  one, 20 messages at a time.
- `send CHAT TEXT` writes to a chat, `--file` adds images (each its own
  message); `send item:<id> TEXT` writes to the item's shop.
- `listen [CHAT…]` prints messages shops send as they arrive.

## Logging in

Taobao serves nothing to guests. `atp taobao login` shows a QR code for
the Taobao app. Taobao may ask for a face or SMS check first: finish it on
the page the error names, then use `atp taobao login --browser`. You can
also log in on www.taobao.com in a browser and run
`atp taobao login --browser`, or pass the cookies with `--cookie` (`unb`,
`cookie2`, `_m_h5_tk`, plus `_nk_`, `cna`, `isg`, `x5sec` as the browser
has them).

## Limits

Taobao's risk control watches direct API calls closely. When it answers
`RGV587` (“被挤爆啦”) or a slider (`verification_required`), open the page
the error names in your browser, pass the check, and import the cookies
again; retrying at once does not help. No orders, carts, coupons or
reviews of your own.
