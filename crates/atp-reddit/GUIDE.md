## Logging in

Reddit has two modes: a browser (cookie) session, used by default, and an
OAuth "script" app set in the environment, which switches everything to
Reddit's OAuth API (`oauth.reddit.com`) and wins over a saved session.

### Browser session

1. Log in at https://www.reddit.com in a local browser.
2. Run `atp reddit login` (the same as `--browser`; Reddit has no QR login)
   or `atp reddit login --browser firefox`. Only `reddit.com` cookies are
   read.

Or copy the `Cookie` header of any request to `www.reddit.com` from the
browser's developer tools and run `atp reddit login --cookie '...'`. It must
hold `reddit_session`.

Reads go to the website's JSON pages with these cookies. Writes use the web
app's own bearer token, the `token_v2` cookie (a JWT living about a day);
when it is missing or expired, ATP loads the home page once to get a new
one. Without a usable `token_v2`, writes fall back to the classic API with
the account's modhash, which refuses some submissions with `BAD_CAPTCHA`;
ATP then retries once with a `token_v2` if one has turned up.

### OAuth script app

Create an app of type "script" at https://www.reddit.com/prefs/apps and set
all four variables (no `login` needed):

- `REDDIT_CLIENT_ID`: the app's client id.
- `REDDIT_CLIENT_SECRET`: the app's secret.
- `REDDIT_USERNAME`, `REDDIT_PASSWORD`: the account (the password is taken
  as typed, spaces included).

ATP asks for a password-grant token, keeps it in the private cache until
shortly before it expires (Reddit says how long; one hour when it does
not), and sends the User-Agent `cli:all-the-platforms:<version> (by
/u/<username>)`. Bad app credentials or a wrong password fail with
`not_authenticated` ("Reddit refused the OAuth credentials"); a token that
Reddit refuses later is dropped, and the next run asks for a new one.

The author's post insights page (`insights POST`) is read from the website
and needs a browser session.

### Without logging in

Search, `hot`, `feed` (r/popular), `read`, `comments`, `reposts`, `user`,
`user-posts`, `download`, `listen r/SUB` / `u/USER` and most Reddit-only
reading commands (`sub`, `subreddit`, `subreddits`, `search-in`, `domain`,
`rules`, `mods`, `wiki`, `multi`, `user-comments`, and `multis`,
`trophies`, `moderated` with a user) work anonymously. Your lists, the
inbox, messages, insights and every write need a session.

Reddit refuses anonymous API requests from many networks: commands then
fail with `verification_required` ("blocked by network security"). Log in,
set the OAuth variables, or retry through another network with `--proxy`.

### When the session ends

How long a `reddit_session` cookie lasts is not known. An expired one makes
commands fail with `not_authenticated` ("Reddit did not accept this
session", or "... the credentials (HTTP 401)"); log in again.

### Environment

- `REDDIT_CLIENT_ID`, `REDDIT_CLIENT_SECRET`, `REDDIT_USERNAME`,
  `REDDIT_PASSWORD`: the OAuth script app above; all four are needed.
- `ATP_REDDIT_COOKIE`: a cookie header for one run, never saved.

## References

- Posts: the id (`1wt65ox`), `t3_<id>`, links on www / old / new / np / m /
  sh.reddit.com, `redd.it/<id>`, `/gallery/<id>`, `v.redd.it/<id>` and
  share links (`/r/<sub>/s/<code>`; short links are followed to the post).
- Comments: `t1_<id>` or a comment permalink. Votes (`like`, `unlike`,
  `downvote`), saves (`favorite`, `unfavorite`), `edit`, `flag`, `approve`
  and `remove` take a post or a comment: a bare id is a post, `t1_<id>` or
  a comment permalink a comment.
- Subreddits: `rust`, `r/rust` or a link; `u/<name>` names a user's profile
  (`u_<name>`). Users: `spez`, `u/spez` or a link.
- Message threads: `t4_<id>` or a message link (`/message/messages/<id>`);
  inbox items: `t1_…` / `t4_…` ids from `notifications`.
- Multireddits: `u/USER/m/NAME`, `USER/NAME` or a link.
- `#N` picks from the last list: a post, a wiki page after `wiki`, a flair
  template after `flairs`, a multireddit after `multis`.

## Commands

### search

Searches posts; `--filter` takes a time range. `search --type user` finds
accounts, `--type topic` subreddits.

### hot

`--category popular` (default) or `all`: the hot posts of r/popular or
r/all.

### feed

Logged in: your front page (`best` by default). Otherwise r/popular, where
`best` does not exist (`hot` by default).

### read

Shows a post without its comments; polls show their options, vote counts
(when Reddit reveals them), total, end time and your choice in
`extra.poll`.

### comments

`--all` expands every "load more" and "continue this thread".

### reposts

Lists crossposts, newest first; other submissions of the same link are left
out. There is no `likers` (votes are private).

### favorites, likes

Your saved and upvoted posts; Reddit shows them to their owner only.
Saved posts have no folders.

### collections

The subreddits (and followed profiles) of the logged-in account; Reddit
does not list other people's.

### like, unlike, downvote

`like` upvotes, `downvote` votes down; `unlike` and `downvote --undo` clear
the vote.

### follow, unfollow

`follow r/<sub>` subscribes to a subreddit; `follow u/<user>` (or a bare
name) follows a user's profile.

### post

`post "text" --title T --topic r/<sub>` (or `--topic u/<you>` for your
profile). A title and a `--topic` are required.

- A single URL as the text makes a link post.
- `-i` images make an image post (one) or a gallery (up to 20).
- `--quote POST` makes a crosspost.
- `--reply-to POST` comments on that post instead.

Crossposts and image posts take no text.

### edit

`edit THING "text"` replaces the text of your own post or comment.

### notifications, unread

Listing the inbox never marks anything read. `unread` counts the newest
100 unread items by kind.

### mark-read

`mark-read` marks the whole inbox read; `mark-read ID… [--unread]` only
those items.

### chats, messages, send

Private messages. `chats` lists your message threads, `messages THREAD`
shows one, `send u/USER "text"` starts a conversation (the first line,
up to 100 characters, is the subject), `send r/SUB "text"` writes to its
moderators, and `send THREAD "text"` (or `--reply-to MESSAGE`) answers.
Messages are text only. Reddit's newer chat (the chat icon) is a separate
system without a public API, so it is not covered.

### listen

`listen [r/SUB|u/USER|inbox …]` polls about every 30 s and prints new
posts, comments or inbox items; with no argument it follows your inbox
(logged in only). The first poll only notes what is already there.

### insights

- `insights [--days N]`: karma (post, comment, awarder, awardee) with
  per-subreddit breakdowns, followers, account age and trophies, plus daily
  numbers computed from your own posts and comments created in the window
  (their current scores, counted on their creation day; up to 1,000 of
  each are read). `extra.native` / `extra.computed` say which is which.
- `insights POST`: public counters (score, upvote ratio, comments,
  crossposts, awards, views when sent); for your own posts also the
  author-only post insights page, which Reddit keeps for about 90 days and
  not for removed or deleted posts.

### download

Reddit videos (`v.redd.it`) keep video and audio apart; ATP adds the audio
track from the video's manifest. A crosspost without media of its own
downloads the original's.

### sub

`sub SUBREDDIT [--sort hot|new|top|rising|controversial] [--time …]`
browses a subreddit; `--time` ranges `top` and `controversial`.

### subreddit, subreddits, rules, mods

`subreddit NAME` shows a subreddit's details; `subreddits
[popular|new|default]` lists communities; `rules SUB`; `mods SUB` lists the
moderators and their permissions.

### search-in, domain

`search-in SUB QUERY` searches one subreddit (Reddit's search syntax);
`domain github.com` lists posts linking a site.

### wiki

`wiki SUB` lists the pages; `wiki SUB PAGE` shows one page as Markdown
(`index` is the main one, `#N` after the list).

### multis, multi

`multis` lists your multireddits (logged in), `multis USER` a user's public
ones; `multi u/USER/m/NAME` (or `#N` after `multis`) browses one.

### flairs, flair

`flairs SUB` lists post flairs (logged in); `flair POST [TEXT] --template
ID` (or `--template '#N'`) sets your post's flair. Give a template, a text
or both; a text only works where the subreddit or template allows editing.

### flag, approve, remove

`flag THING nsfw|spoiler|replies|lock|sticky [--undo]`: NSFW, spoiler and
inbox replies are yours to set on your own posts; lock and sticky need a
moderator (sticky takes posts only), as do `approve THING` and `remove
THING [--spam]`.

### saved-comments, hidden, downvoted

Your other lists, beside `favorites` (saved posts) and `likes`.

### hide, block

`hide POST [--undo]` hides a post from your listings; `block USER
[--undo]`.

### user-comments, trophies, moderated

`user-comments USER` lists a user's comments; `trophies [USER]`;
`moderated [USER]` the subreddits they moderate. Without USER they show
yours.

## Limits

- Requests are spaced at least 1 s apart, and each write waits another
  0.6-1.8 s.
- HTTP 429 or a `RATELIMIT` answer gives `rate_limited`; wait a minute.
  Logged-in requests get a higher limit.
- Private, quarantined, gated and Premium-only communities answer
  `permission_denied`; open a quarantined or gated one on the website once
  (opt in), then log in here.
