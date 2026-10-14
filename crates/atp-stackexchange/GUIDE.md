Stack Overflow and the other sites of the Stack Exchange network (Super
User, Server Fault, Ask Ubuntu, Mathematics …) through the official
Stack Exchange API. Commands work on one site at a time: the site of a
link you pass, else `SE_SITE` (`SE_SITE=superuser atp so hot`), else the
site chosen with `atp so site NAME`, else Stack Overflow. A site is named
by its short name (`superuser`, `math`) or its domain
(`math.stackexchange.com`); `atp so sites` lists them all.

## Logging in

Everything you can read works without an account. Logging in means
registering an app of your own on Stack Apps, which gives a key (more
requests a day) and, through its OAuth settings, an access token (your
inbox and writing).

### Without an account

Search, `hot`, `read`, `comments`, `replies`, `user`, `user-posts`, `tags`,
`tag`, `sites` and `quota` need nothing. Stack Exchange allows an IP 300
requests a day without a key; each command makes one request per page
(`read` makes one or two). `atp so quota` shows what is left.

### App key

1. Sign in on https://stackapps.com and register an app at
   https://stackapps.com/apps/oauth/register (name, description, OAuth
   Domain and Application Website are required; for the token login below
   use `localhost` as the OAuth Domain and tick "Enable Client Side OAuth
   Flow").
2. The app page shows a **Client Id** (a number), a **Client Secret** and a
   **Key**. ATP never needs the secret.
3. `atp so login --token 'KEY'` keeps the key. It is checked with one
   request and raises the daily quota to 10,000 requests. The key is not a
   secret in Stack Exchange's terms, but it is still kept only in ATP's
   session file (readable by you only).

`STACKEXCHANGE_KEY` supplies a key for one run instead.

### Access token (browser)

`atp so login --token 'CLIENT_ID:KEY'` runs the implicit OAuth flow:

1. ATP listens on `127.0.0.1:8898` (`SE_REDIRECT_PORT` picks another port)
   and opens `https://stackexchange.com/oauth/dialog` in your default
   browser; the link is printed too.
2. Sign in there and approve. The app asks for `read_inbox` (the
   `notifications` command), `write_access` (votes, comments, answers,
   bookmarks) and `no_expiry`.
3. Stack Exchange sends the browser back to
   `http://localhost:8898/callback` with the token after `#`; a small page
   hands it to ATP, which saves it with the key. The login waits five
   minutes for this.

The redirect has to fall under the app's OAuth Domain, so the app must use
`localhost` there and allow the client side flow. Whether Stack Exchange
accepts every port on `localhost` is not documented; if it refuses the
redirect, get a token by hand as below.

### Access token (by hand)

Open, with your Client Id:

    https://stackexchange.com/oauth/dialog?client_id=CLIENT_ID&scope=read_inbox,write_access,no_expiry&redirect_uri=https://stackexchange.com/oauth/login_success

approve, and copy the `access_token=…` value from the address the browser
lands on (the page Stack Exchange documents for desktop apps). Then
`atp so login --token 'KEY:ACCESS_TOKEN'`; `STACKEXCHANGE_ACCESS_TOKEN`
supplies one for a single run. A token is used only together with its
app's key.

### How long it lasts

A `no_expiry` token works until you remove the app's access in your Stack
Exchange account settings; a token without it lasts a day. A token that
stops working fails with `not_authenticated` (`invalid_access_token`), and
`atp so login` again fixes it.

### Limits and pacing

ATP spaces requests at least a second apart. When an answer asks to back
off for some seconds, the next request (in this run or the next) waits;
waits over 30 seconds fail with `rate_limited` instead. A used-up daily
quota, or too many requests too fast, fails with `rate_limited`
(`throttle_violation`). Below 20 requests left, a note says how many
remain. Writes wait one to two and a half seconds first; the sites limit
how often an account may comment or answer and need reputation for votes
and comments (their privileges pages say how much), and refuse with an
`upstream_error` that quotes the reason (`write_failed`).

### Environment

`STACKEXCHANGE_KEY` and `STACKEXCHANGE_ACCESS_TOKEN` (win over the saved
ones, never saved), `SE_SITE` (the site for one run), `SE_REDIRECT_PORT`
(the browser login's port).

## References

- Questions: a link (`https://stackoverflow.com/questions/11227809/…`,
  `https://superuser.com/q/12345`), `q/<id>`, or the id.
- Answers: a link (`…/questions/<q>/<slug>/<answer>`, `…#<answer>`,
  `/a/<id>`), `a/<id>`, or the id. A bare id is tried as a question
  first, then as an answer.
- Comments: the id, or a link ending in `#comment<id>_<post>`.
- Users: a profile link (`https://stackoverflow.com/users/22656/jon-skeet`),
  `users/<id>`, the id, or `me` (needs an access token). User ids differ
  from site to site.
- A link carries its site; ids are looked up on the default site. `#N`
  works after any list.

## Commands

### search

Questions through the advanced search. Words in brackets are tags that
must all be present: `atp so search '[rust] [async] lifetime'`; a query of
only tags lists their questions. `--sort relevance|votes|newest|active`,
`--filter accepted|unanswered|closed|open`. `search -t user` finds users
by name (highest reputation first); `search -t topic` finds tags.

### hot

`--category hot|week|month|featured|unanswered|newest|active`: the site's
hot questions (the default), the week's and month's, questions with a
bounty, unanswered ones by votes, the newest, the recently active.

### read

A question as on its page: the body, its comments, then every answer
(highest score first, the accepted one marked) with their comments, as
plain text rendered from the posts. `extra` has the score (negative ones
too), the accepted answer and the closing reason. An answer reads the
same way with its comments.

### comments

On a question: its answers, as comments (score as likes,
`extra.accepted`), each with the comments under it as replies.
`--sort votes|newest|oldest|active` (votes first by default). On an answer:
the comments under it.

### replies

`replies QUESTION ANSWER` lists the comments under the answer;
`replies QUESTION QUESTION` the ones under the question itself.

### user, user-posts

A user of the site, with reputation, badges, the profile text and when
they were last seen. `user-posts` lists their questions and answers, newest
first.

### notifications

Your inbox across all sites (comments, new answers, chat mentions …);
needs the access token with `read_inbox`.

### like, unlike

`like` upvotes a question or answer, `unlike` takes the vote back. Needs
an access token and, on Stack Overflow, 15 reputation.

### downvote

`downvote POST [--undo]`; on Stack Overflow it takes 125 reputation.

### favorite, unfavorite

`favorite` bookmarks a question (the API still calls bookmarks
favorites), `unfavorite` removes the bookmark.

### comment

Writes a comment under a question or answer. Comments are flat:
`--reply-to COMMENT` starts the text with `@Name` of that comment's
author.

### delete-comment

Deletes a comment of yours.

### answer

`answer QUESTION TEXT` posts an answer; the text is Markdown (`-` reads it
from stdin).

### tags, tag

`tags [WORDS]` lists the site's tags, most used first; `tag TAG` lists its
questions (`--sort active|newest|votes|hot|week|month`; `a;b` needs both
tags).

### site, sites

`site` shows the default site and where it comes from; `site NAME` saves
a new default. `sites` lists the network's sites; their `id` is the name
to use.

### quota

How many requests are left today for this IP (or app key), out of how
many.

## Limits

No asking or editing questions, no accepting answers, no flags, chat or
reviews. Comments under a question show in `read` and through `replies`,
not in `comments`. Search finds questions only (not answers by text). The
inbox has no `--type` filter and no unread counter.
