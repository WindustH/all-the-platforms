## References

- Items: `https://www.goofish.com/item?id=<id>`, the old
  `2.taobao.com/item.htm?id=<id>`, `m.tb.cn/…` share links, share text
  with such a link in it, or a bare item id.
- Users: `https://www.goofish.com/personal?userId=<id>`, a bare user id,
  or `me`.
- Chats: the ids `chats` lists. `send` also takes `item:<id>` or an item
  link: it opens the chat with that item's seller (or finds the one there
  is).

## Reading

- `search` (`--sort relevance|newest|price-asc|price-desc`), 30 items a
  page. Items show their price in `extra.price` (yuan, as shown), the
  original price, the location and the seller.
- `read ITEM` shows the description, images (and video), views, wants,
  favorites, condition and other properties; `download ITEM` saves the
  media.
- `user USER` (followers, items, ratings, credit tags),
  `user-posts USER` (the items a user sells) and `ratings USER` (the
  ratings a user received, `extra.rate` good/neutral/bad, `extra.from`
  saying whether a buyer or a seller gave it).
- `feed` (recommended items) and `favorites` (your own).
- `hot`: the words the search box rotates through as hints;
  `suggest KEYWORD`: the words it suggests for a keyword.
- `hot` and `suggest` work without a login; everything else needs one.

## Chats

- `chats` lists your chats, the most recently active first; each is about
  an item (`description`) with one other person (`extra.peer`).
- `messages CHAT` pages back through a chat, 20 messages at a time.
- `send CHAT TEXT` writes to a chat; `--file` adds images (each sent as
  its own message). `send item:<id> TEXT` writes to the item's seller.
- `listen [CHAT…]` prints messages others send as they arrive.
- Use `send` for single messages you mean to write: Xianyu restricts
  accounts that send automated or bulk messages.

## Logging in

`atp xianyu login` shows a QR code for the Xianyu app. Xianyu may ask for a
face or SMS check first: finish it on the page the error names, then use
`atp xianyu login --browser`. `--cookie` works too (`unb`, `cookie2`,
`_m_h5_tk`).

## Limits

When Xianyu suspects automation it answers `RGV587` (“被挤爆啦”) or asks for
a slider (`verification_required`): wait a few minutes, or open
www.goofish.com in a browser, pass the check and import the cookies again.
No publishing, editing or ordering; items have no public reviews.
