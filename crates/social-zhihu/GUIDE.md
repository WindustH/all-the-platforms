## References

- Links: `https://www.zhihu.com/question/<q>`, `…/question/<q>/answer/<a>`,
  `https://www.zhihu.com/answer/<a>`, `https://zhuanlan.zhihu.com/p/<id>`
  (article), `https://www.zhihu.com/pin/<id>` (pin, 想法),
  `https://www.zhihu.com/zvideo/<id>` (video).
- Typed ids: `q:<id>`, `a:<id>`, `article:<id>`, `p:<id>` (pin), `v:<id>`
  (video); a bare number is an answer.
- Users: the url_token (`zhang-jia-wei`), `@token`, or a `/people/` link.
- Topics and columns: the id, a link, or `#N` after a topic / column list.

## Logging in

`social zhihu login` shows a QR code for the Zhihu app. Zhihu refuses almost
everything to anonymous clients (only `hot` works) and flags networks that
keep trying; when the QR poll reports risk control, pass the check in the
browser or use `social zhihu login --browser`.

## Analytics

`social zhihu insights [--days N]`: the creator center.

- totals and daily series: `views`, `impressions`, `ctr`, `completion_rate`,
  `likes` (赞同), `hearts` (喜欢), `reactions`, `comments`, `favorites`,
  `shares`, `reposts`, follower gains and losses, `profile_visitors`.
- breakdowns: `traffic_source`, reader `gender` / `age` / `region` (lifetime),
  `content_type`, and a follower portrait once Zhihu builds one.

`social zhihu insights <your answer, article or pin>`: the same for one post
plus followers it brought. Others' posts get public counters.

`social zhihu creations -t answer|article|pin` lists your posts with their
lifetime numbers.

## Reading

- `feed --type follow` is the 关注 feed (who did what is in
  `extra.action_text` and `extra.actor`); the default is 推荐.
- `history` lists what you read (`extra.read_at`).
- `notifications --type comment|like|favorite|follow|message`: the app's
  timelines; `message` lists private-message threads. `all` (default) is
  the web's recent list.
- `activities [USER]` (动态), `user-questions`, `user-videos`, `user-columns`
  and `user-following [USER] -t questions|topics|columns|folders`; without
  USER they show your own.
- `question-followers QUESTION`, `topic-followers TOPIC`, `column COLUMN
  [--info]` (articles of a 专栏).
- `hot-search` lists trending searches (热搜); `daily [--date YYYYMMDD]`
  reads Zhihu Daily (知乎日报), no login needed.
- Videos (`zvideo`) and video pins carry their best stream for `download`.

## Interactions

- `likers` lists upvoters of answers, articles and pins.
- `answers QUESTION [--sort default|created]`, `topic ID [--essence]`,
  `user-articles USER`, `user-pins USER`.
- `post` publishes a pin (想法) with optional `--title` and images; `ask
  TITLE [-d DETAIL] [-t TOPIC] [-i IMAGE]` asks a question.
- `answer QUESTION BODY [-m] [-i IMAGE]` answers a question and `article
  TITLE BODY [-m] [-t TOPIC] [-i IMAGE]` publishes a column article; `-` as
  BODY reads it from stdin: `social zhihu answer q:123 - -m < answer.md`.
- `follow-question QUESTION [--undo]`.
- `vote ANSWER up|down|neutral` (反对 is `down`), `like-comment COMMENT
  [--undo]`, `block USER [--undo]`.
- `create-folder TITLE [-d TEXT] [--private]` and `delete-folder FOLDER -y`
  manage favorites folders (`collections` lists them).
- `delete` removes your own pin, article, question or answer.

With `-m / --markdown` the body is read as Markdown: `#` / `##` headings
(deeper levels become bold), `-` and `1.` lists, `>` quotes, fenced code,
`|` tables, `---` rules, `**bold**`, `` `code` ``, `[text](url)` links, an
image alone on its line (`![caption](path)`, uploaded and placed there), and
a link alone on its line with the title `"card"` (`[text](url "card")`) as a
link card. Without it, every line becomes a paragraph.

## Limits

Questions and topics are read from their pages, and topic feeds other than
`--essence` are not offered: those API endpoints need the `x-zse-96`
signature. The hot list has no categories any more (Zhihu serves the full
list for every category name).
Reposts have no public list.
