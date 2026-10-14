## Logging in

- **QR code**: `atp xianyu login` shows the QR code of Xianyu's web login
  (passport.goofish.com). Scan it with the Xianyu (闲鱼) app and confirm
  there. A code that expires or is cancelled ends the login; run it again.
- **Face or SMS check**: Xianyu may want to confirm a new login first. The
  QR login then fails with `verification_required` and the page to do the
  check on (`error.url`): finish the check there in a browser, then run
  `atp xianyu login --browser`. When Xianyu names no page, log in on
  www.goofish.com in a browser and import that session.
- **Browser**: log in on https://www.goofish.com, then
  `atp xianyu login --browser` reads that browser's goofish.com cookies.
- **Cookie string**: `atp xianyu login --cookie '...'` takes the `Cookie`
  header of a request to www.goofish.com (developer tools, Network tab).
  `unb` (the user id) and `cookie2` are required. `_m_h5_tk`, the token
  every call is signed with, may come along; ATP fetches a fresh one when
  it is missing or expired.
- `ATP_XIANYU_COOKIE` supplies such a cookie header for one run; it is
  never saved.

Without a login only `hot` and `suggest` (the search box's words) answer;
Xianyu walls every other call from visitors (`RGV587_ERROR::SM`).

How long a session lasts is not known. When it ends, commands fail with
`not_authenticated` ("Xianyu wants a login") and a hint naming the ways to
log in. Chats use an access token of their own that ATP gets with the
session (it lasts a day unless Xianyu says otherwise) and renews by
itself; `listen` keeps the session alive every 10 minutes, as the web chat
does.

### Risk control

- When Xianyu suspects automation it answers `RGV587` ("被挤爆啦") or asks
  for a slider: `verification_required`, with the page in `error.url`. On
  a terminal ATP opens it in a browser (see `atp guide login`). Otherwise
  wait a few minutes, or open www.goofish.com in a browser, pass the check
  and import the cookies again. A wall that leads to the login page is
  `not_authenticated` instead.
- Traffic limits (`FAIL_SYS_TRAFFIC_LIMIT`, `FLOWLIMIT`) are
  `rate_limited`. ATP waits 1.5 seconds between requests.
- Xianyu restricts accounts that send automated or bulk messages: use
  `send` for single messages you mean to write.

## References

- Items: `https://www.goofish.com/item?id=<id>`, the old
  `2.taobao.com/item.htm?id=<id>`, `m.tb.cn/…` share links, share text
  with such a link in it, a bare item id, or `#N` after a list.
- Users: `https://www.goofish.com/personal?userId=<id>`, a bare user id,
  or `me`.
- Chats: the ids `chats` lists (`<id>@goofish` works too), or `#N` after
  `chats`. `send` also takes `item:<id>` or an item link: it writes to that
  item's seller.

## Commands

Items are posts of `kind` `item` with their price in `price` (yuan,
`price.original` the original price), `extra.status` (`on_sale`, `sold`,
`off_shelf`) and `extra.location`.

### search

30 items a page, up to page 50 (the web app pages no further). Each item
names its seller. When nothing matches exactly, Xianyu shows related items
and ATP notes it.

### hot, suggest

`hot`: the words the search box rotates through as hints. `suggest
KEYWORD`: the words it suggests for a keyword. Both work without a login.

### feed

Recommended items of the home page, 30 a page.

### read, download

`read ITEM` shows the description, images (and video), views, wants
(`metrics.other.wants`), favorites, condition and other properties
(`extra.properties`). `download ITEM` saves the images and the video.

### user, user-posts

`user USER`: followers, following, item and rating counts, credit tags
(`extra.tags`) and the good-rating ratio (`extra.praise_ratio`).
`user-posts USER`: the items a user sells, their pinned item first, 20 a
page.

### ratings

The ratings a user received, 20 a page, newest first: `extra.rate` is
`good`, `neutral` or `bad`, `extra.from` says whether a buyer or a seller
gave it, `extra.images` holds its pictures.

### favorites

Your own favorites, newest first; Xianyu shows no one else's.

### chats

Your chats, the most recently active first. A chat is always about one
item (`description`, `extra.item`) with one other person (`extra.peer`);
`extra.unread` and `extra.last_message` come along.

### messages

Pages back through a chat, 20 messages at a time. An item is not a chat:
find the chat about it with `chats`.

### send

`send CHAT TEXT` writes to a chat; `-a FILE` adds images (images only,
each sent as a message of its own). `send item:<id> TEXT` writes to the
item's seller, opening the chat or finding the one there is; writing to
your own item is refused. `--reply-to` is not supported.

### listen

Prints messages others send as they arrive, from every chat or only the
chat ids given (not items). The connection is opened again when Xianyu
drops it.

## Limits

No publishing, editing or ordering, and items have no public reviews.
