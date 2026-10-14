Mastodon on any instance, through its REST API. Every command talks to
one instance: `MASTODON_URL` when set (`MASTODON_URL=fosstodon.org atp
mastodon hot`), else the one you logged in to, else mastodon.social.
Statuses and accounts of other servers are reached through it.

## Logging in

### OAuth in the browser

`atp mastodon login --token INSTANCE` (a host or URL, such as
`mastodon.social`) registers ATP as an app on the instance with the
redirect address `http://127.0.0.1:8897/callback`, opens the
instance's consent page in your browser, and keeps the token it hands
back once you allow access. The token may read and write everything
(`read write`). `MASTODON_REDIRECT_PORT` picks another port when 8897 is
taken. The browser has to run on the same machine as ATP (it comes back
to 127.0.0.1); elsewhere, use a token (below).

### Access token

`atp mastodon login --token 'INSTANCE|TOKEN'` takes a token made on the
instance under Preferences → Development → New application (give it the
`read` and `write` scopes, or fewer for fewer commands); a bare `TOKEN`
is for the instance in use. `-` reads the value from stdin. The token is
checked (`verify_credentials`) before it is saved.

### Environment

`MASTODON_URL` (a host or URL) and `MASTODON_TOKEN` supply the instance
and token for one run; they win over the saved session and are never
saved.

### Without an account

Requests go out anonymously, and the instance answers what it shows the
public: trending statuses, links and hashtags (`hot`, `trends`), hashtag
timelines (`tag`, `search '#tag'`), statuses and their threads (`read`,
`comments`, `replies`, `likers`), accounts, their statuses, followers and
follows, and account and hashtag search. Many instances (mastodon.social
among them) close the local and public timelines to visitors; those
fail with `not_authenticated`. Searching statuses, links to other
servers, the home timeline, notifications, likes and bookmarks, and
every write need an account.

### How long a login lasts

Mastodon tokens do not expire; they end when you revoke the app under
Preferences → Account → Authorized apps, and commands then fail with
`not_authenticated` and the hint to log in again. Instances limit
requests (300 per five minutes per account or IP by default) and
answer `rate_limited` beyond that; writes wait 0.3 to 0.9 seconds first
and requests are spaced at least half a second apart.

## References

- Statuses: the id, or a link on the instance in use (`/@user/<id>`,
  `/@user@host/<id>`, `/users/user/statuses/<id>`). Lists print links on
  the instance in use, so `#N` and printed links work without looking
  anything up; links to other servers are looked up through the
  instance (`resolve`, needs an account).
- Accounts: `@user` (this instance), `@user@host`, a profile link, an id,
  or `me`.

## Commands

### feed

`--type home|local|public|remote`; `home` (the default when logged in)
needs an account, `local` is the default without one.

### hot

`--category statuses|links|local|public`: trending statuses (the
default), trending links, or the local or public timeline.

### search

`search WORDS` searches statuses (needs an account; Mastodon finds only
statuses whose authors allow search, and ones you wrote, liked, boosted
or were mentioned in); `#tag` alone lists the hashtag's timeline, which
works without an account. `search -t user`, `search -t topic`
(hashtags). Logged in, results include accounts and statuses of other
servers (`resolve`), and further pages work.

### trends, tag

`trends` lists the hashtags trending on the instance, with their uses
and the accounts using them over the last week; `tag TAG` the statuses
with one.

### comments, replies

`comments STATUS` lists the whole thread after a status at once
(`reply_to` is the status each one answers); `replies STATUS REPLY` only
the direct replies to one of them.

### likers

The accounts that liked (favourited) a status.

### favorites, likes

`favorites` lists your bookmarks and `likes` your favourites; Mastodon
shows nobody else's.

### like, favorite

`like` favourites a status, `favorite` bookmarks it; `--undo` (or
`unlike`, `unfavorite`) takes it back.

### boost

`boost STATUS [--undo]` boosts (reblogs) a status or takes the boost
back.

### post

`post TEXT [--title CW] [-i FILE ...] [--topic TAG] [--reply-to STATUS]
[--quote STATUS]`: the title becomes the content warning, each topic a
hashtag at the end, files (images, video or audio; four at most) are
uploaded first and ATP waits until the instance has processed them.
`--quote` needs Mastodon 4.5 or later.

### compose

Takes every option: `--cw`, `--visibility public|unlisted|private|direct`
(`direct` with `@user@host` in the text is a private mention),
`--language`, `--sensitive`, `--reply-to`, `--quote`, `-i FILE`.

### comment

Replies to a status (or to `--reply-to REPLY`) with the same visibility
and content warning, mentioning its author first unless the text does
or the status is yours; `-i` attaches files.

### follow

Following a locked account sends a follow request, which the result
says.

### notifications

`--type mention|status|reblog|follow|follow_request|favourite|poll|update|quote`.

### delete, delete-comment

Deletes one of your statuses.

### instance

The instance's name, version, longest status, media per status, whether
sign-ups are open, monthly active users, contact and rules.

## Limits

No editing, polls, lists, filters, scheduled statuses or direct-message
conversations yet (a `compose --visibility direct` status reaches the
accounts it mentions). Trends are what the instance itself counts.
