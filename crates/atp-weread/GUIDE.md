WeRead (微信读书) through the routes weread.qq.com's web reader calls: the
store's search, rankings and categories, and with a login a book's details
and public reviews (点评), your shelf, how far you are in each book, and
your own highlights (划线) and thoughts (想法), which `notes` lists in
reading order or writes as Markdown. ATP never fetches the text of books:
they are paid, protected content. Your notes are your own data.

## Logging in

### QR code

`atp weread login` shows a QR code of weread.qq.com's login. Scan it with
WeChat and confirm. WeRead sometimes asks for the four-digit code WeChat
then shows; ATP asks for it on the terminal (without a terminal, use
`--browser`). The login keeps the cookies the web reader keeps: `wr_vid`
(your user number), `wr_skey` (a short-lived key), `wr_rt` (the token that
renews it) and `wr_fp` (a random id of this install, sent as the site
sends its browser fingerprint, so that logging in here does not end your
other WeRead web sessions).

### Browser or cookie

- `atp weread login --browser` reads the cookies of a browser logged in at
  weread.qq.com.
- `atp weread login --cookie '...'`: the `Cookie` header of a request to
  weread.qq.com. It must hold `wr_vid` and `wr_skey`, and should hold
  `wr_rt`, without which the session cannot renew itself (below).
- `ATP_WEREAD_COOKIE='...'` passes such a header for one run; it is never
  saved.

There are no tokens and no other environment variables.

### Without an account

`search`, `hot` and `category` work without logging in. Everything else
(`read`, `comments`, `user`, `favorites`, `notes`, `progress`, `stats`)
needs one: WeRead answers visitors `-2010 用户不存在`.

### How long it lasts, and how its end shows

`wr_skey` is short-lived (the web reader renews it all the time; its
lifetime is not published). Once it has expired, WeRead answers `-2012
登录超时`; ATP renews the key with `wr_rt` through `/web/login/renewal`,
as the web reader does, keeps the new cookie and repeats the request, so
this is usually invisible. How long `wr_rt` itself lasts is not published; once it
has expired too, or the session was ended elsewhere, commands fail with
`not_authenticated` ("the session expired and could not be renewed",
`-2013 鉴权失败` or `-12013`) and the hint to log in again. A cookie
imported without `wr_rt` fails as soon as its key expires.

### Risk control

No captcha flow is known on these routes. ATP keeps at least 0.8 s
between requests and does not write anything to your account.

## References

- Books: the book id (`695233`; official-account books `MP_WXS_…`), a
  reader link (`https://weread.qq.com/web/reader/<hash>`, also with a
  chapter), a detail link (`https://weread.qq.com/web/bookDetail/<hash>`),
  a share link (`https://weread.qq.com/book-detail?v=<hash>`), or `#N`.
  The hash is decoded to the id and checked.
- Users: the user number (`vid`, as `whoami` shows it).

## Commands

### search

The store's search (books, also web novels and audiobooks); each result
has its place, readers, rating and price.

### hot

A ranking by `--category`: `rising` (飙升榜, the default), `newbook`
(新书榜), `novel` (小说榜), `all` (总榜), `newrating` (神作榜), `potential`
(神作潜力榜), `hot-search` (热搜榜). Each book carries its place
(`extra.rank`), readers (`metrics.readers`), the share of readers who
recommend it (`extra.recommend_percent`) and WeRead's verdict
(`extra.rating_title`, such as 神作).

### read

A book's title, author, translator, publisher, introduction, category,
ISBN, words, price, rating and publication date.

### comments

Public reviews (点评) of a book, in the order WeRead lists them, with the
reviewer's verdict (`extra.rating`: recommend, fair or poor) and stars
where given. There is no order to choose.

### user

A reader by number: name, avatar, signature.

### favorites

Your shelf: books, audio albums (`kind` `album`) and official accounts
(`mp`), the most recently read first, each with its progress in percent
(`extra.progress`) and reading time (`extra.reading_seconds`) where the
shelf says.

### notes

`notes` lists the books you highlighted or wrote thoughts in, the latest
first, with their counts. `notes BOOK` lists your highlights (`kind`
`highlight`) and thoughts (`thought`, with the passage in `extra.quote`)
plus your review of the book (`review`), in reading order, each with its
chapter. `notes BOOK --markdown FILE` writes them as Markdown instead:
chapters as headings, highlights and passages as quotes, thoughts as
text.

### progress

`progress BOOK`: the percent read, the chapter, the reading time in
seconds, when you started, finished and last read, and the text near
your position.

### stats

Totals of your shelf (books, albums, finished, started, reading time
where the shelf says) and of your notes.

### category

`category ID`: the books of a store category (`100000` 精品小说, `200000`
历史, `300000` 文学, `700000` 计算机, …; the ids appear in the store's
links) or of a ranking by its id.

## Limits

No book contents, no audiobook playback, no writing (no adding to the
shelf, highlights, thoughts or reviews), no friends' rankings or reading
reports (周报 / 年报) beyond `stats`, which only adds up what the shelf
and notes report: WeRead's detailed reading-time statistics are only in
the app and its API-key skill, which ATP does not use.
