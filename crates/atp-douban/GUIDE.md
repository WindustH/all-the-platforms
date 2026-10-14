Douban (豆瓣) through the JSON API that m.douban.com's own pages call:
films, TV shows, books and albums with their ratings, short comments (短评)
and reviews (影评 / 书评 / 乐评), Douban's lists (Top 250, 一周口碑榜, 实时热门
…), the 新片榜 chart, groups (小组) with their topics and replies, and users
with what they marked (想看 / 在看 / 看过). A login adds your own marks and
`mark`, which marks a subject 想看 / 看过 with stars.

## Logging in

### QR code

`atp douban login` shows the QR code of Douban's web login page
(accounts.douban.com). Scan it with the Douban app and confirm on the
phone. The login keeps the `dbcl2` and `ck` cookies Douban sets.

### Browser or cookie

- `atp douban login --browser` reads the cookies of a browser logged in at
  douban.com.
- `atp douban login --cookie '...'`: the `Cookie` header of a request to
  m.douban.com or www.douban.com. It must hold `dbcl2` (the session) and
  `ck` (the token Douban's forms carry; `mark` needs it).
- `ATP_DOUBAN_COOKIE='...'` passes such a header for one run; it is never
  saved.

There are no tokens and no other environment variables.

### Without an account

`search`, `hot`, `read`, `comments`, `user`, `download`, `reviews`,
`group`, `list` and `library USER` work without logging in, for what
Douban shows visitors. Groups that only members may read, and users who
hide their marks, answer `permission_denied`.

### With an account

`whoami`, `library` without a user (your own marks) and `mark`. `read`
of a subject then also shows your own mark in `extra.interest`.

### How long it lasts

How long a Douban session lasts is not published. When it ends, commands
that need it fail with `not_authenticated` and the hint to log in again.

### Risk control

- Douban limits how often one address or account may ask. When it wants
  a human check (`sec.douban.com`, a captcha, or a "系统繁忙" spam answer),
  commands fail with `verification_required` and the page to open; on a
  terminal ATP opens it in your Chrome with the session's cookies, you
  pass it by hand, and the command runs again. Slow down with
  `--interval` before that happens.
- ATP keeps at least 1.5 s between requests.
- Writes are the risky part: Douban restricts or bans accounts that mark
  or comment in bursts or in patterns that look automated. `mark` waits 3
  to 8 s before it sends and changes one subject per run; do not loop it
  over long lists.

## References

- Subjects: `https://movie.douban.com/subject/<id>/` (films and TV shows),
  `https://book.douban.com/subject/<id>/`, `https://music.douban.com/subject/<id>/`,
  `https://m.douban.com/<movie|tv|book|music>/subject/<id>/`,
  `douban://douban.com/movie/<id>` app links, `movie:<id>`, `tv:<id>`,
  `book:<id>`, `music:<id>`, or a bare id (a film or show).
- Reviews: `https://movie.douban.com/review/<id>/` (any site's
  `/review/<id>/`) or `review:<id>`.
- Groups: `https://www.douban.com/group/<id or name>/` or `group:<id>`;
  group topics: `https://www.douban.com/group/topic/<id>/` or `topic:<id>`.
  Douban serves a topic only with the `_spm_id` its own lists put in the
  topic's link (`…/topic/<id>/?_spm_id=…`), so use the link or `#N` that
  `group` lists; a bare topic id usually answers `permission_denied`.
- Users: `https://www.douban.com/people/<uid>/`, `@uid`, the uid or the
  user number.
- Lists: `https://m.douban.com/subject_collection/<id>` or the id.
- `#N` works after any list.

## Commands

### search

The mobile site's search, ten results a page. `--filter movie` (films and
TV shows, the default), `book`, `music` or `group`. Results carry the
score in `extra.rating`; `read` one for everything else.

### hot

Douban's lists by `--category`: `hot` (实时热门书影音, the default),
`movie` (实时热门电影), `top250` (豆瓣电影 Top 250), `weekly` (一周口碑电影榜),
`new` (新片榜, from movie.douban.com/chart, one page), `showing` (影院热映),
`latest` (近期热门电影), `tv` (实时热门剧集), `tv-chinese` (华语口碑剧集榜),
`tv-global` (全球口碑剧集榜), `book-top250` (豆瓣读书 Top 250), `fiction` /
`nonfiction` (虚构 / 非虚构类图书热门榜), `music` (热门单曲榜). Each entry
has its place in `extra.rank`. Other lists: `list`.

### read

A subject shows its title (the original in `extra.original_title`), the
introduction, type, release date, score and votes (`extra.rating`,
`extra.votes`), directors, actors, authors or singers, countries, length,
honours such as `豆瓣电影Top250 #1` (`extra.honor`), genres and tags, the
cover, and counts of short comments and reviews. A review shows its whole
text and its stars (1–5); a group topic its text, photos and group; a
group its introduction and member count.

### comments

Of a subject: its short comments (短评) with stars (`extra.rating`, 1–5)
and the reader's state (`extra.status`), `--sort hot` (the default) or
`latest`. Of a review or group topic: its replies; a review's replies
carry the replies under them, topic replies name the reply they answer
(`reply_to`).

### user

A profile: name, uid (handle), introduction, location, followers,
following, the number of films, books and albums marked and reviews
written.

### reviews

`reviews SUBJECT [--sort hot|latest]`: reviews with their title, abstract,
stars, useful votes (`likes`) and reply count. `read` one for the whole
text.

### group

`group GROUP`: the group's topics, pinned ones first (`extra.sticky`),
then the newest. `read '#N'` shows a topic's text and `comments '#N'` its
replies (see References for why `#N` or the listed link).

### list

`list ID`: any Douban list of the `subject_collection` kind
(`m.douban.com/subject_collection/<id>`), such as a chart shared from the
app, in order, with the place in `extra.rank`.

### library

`library [USER] [--type movie|book|music] [--status wish|doing|done]`:
what a user marked, the latest first (`done` and `movie` by default;
`movie` includes TV shows). Each subject carries the mark in
`extra.collection` (state, stars, short comment, tags, time) and its time
is when it was marked, so `--since 30d` shows recent marks. Without USER,
your own (needs a login).

### mark

`mark SUBJECT wish|doing|done [--rating 1-5]`: marks a subject as the
subject page's form does (needs a login with `ck`). `doing` is for shows,
books and albums only; films are `wish` or `done`. An existing short
comment, tags and stars are kept unless `--rating` replaces the stars.
See the ban risk under Logging in.

## Limits

No posting of short comments, reviews or group replies, no removing a
mark, no 豆列 (doulists), notes, statuses (广播) or events, and no
celebrity pages. The subject API's signed app routes (Frodo) are not used,
so data only the app shows is missing. Search pages are ten results long
and only one type at a time.
