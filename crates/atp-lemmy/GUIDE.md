Lemmy, the federated link aggregator, on any instance through its HTTP
API v3 (Lemmy 0.19). Every command talks to one instance:
`LEMMY_INSTANCE` when set (`LEMMY_INSTANCE=lemmy.ml atp lemmy hot`), else
the one you logged in to, else lemmy.world. Posts, comments, communities
and users of other instances are reached through it.

## Logging in

### Password

`atp lemmy login --token 'INSTANCE|USER|PASSWORD'` logs in on the
instance (`USER` may also be the account's email) and keeps the login
token it hands out, with the instance. Pipe the value to keep the
password out of the shell history:

    printf '%s' 'lemmy.world|you|…' | atp lemmy login --token -

The parts are separated by `|` only, so the password may contain `|`.
Accounts with two-factor login need the current code in `LEMMY_TOTP`
for that run (`LEMMY_TOTP=123456 atp lemmy login --token -`). An account
whose sign-up or email verification is still pending cannot log in.

### Login token

`atp lemmy login --token 'INSTANCE|JWT'` keeps a login token you already
have (the `jwt` cookie of the instance's web page); a bare `JWT` is for
the instance in use. The token is checked (`whoami`) before it is saved.

### Environment

`LEMMY_INSTANCE` (a host or URL) and `LEMMY_TOKEN` (a JWT) supply the
instance and token for one run; they win over the saved session and are
never saved. `LEMMY_TOTP` is read by password logins only.

### Without an account

Everything public works anonymously: `hot`, `feed --type local|all`,
`search`, `read`, `comments`, `replies`, `user`, `user-posts`,
`community`, `community-info`, `communities` and `instance`. Your
subscribed feed, saved posts, replies, mentions, unread counts and every
write need an account; without one they fail with `not_authenticated`.
Links of other instances are looked up through yours
(`resolve_object`), which some instances allow only to accounts.

### How long a login lasts

A login token stays valid until you log out on the web or change your
password; when the instance stops accepting it, commands fail with
`not_authenticated`. Lemmy instances limit requests per IP with their own
settings, which ATP cannot see; beyond them commands fail with
`rate_limited`. ATP spaces requests at least half a second apart and
waits 0.4 to 1.2 seconds before each write. Requests name ATP as their
client (`User-Agent`).

## References

- Posts: `https://<instance>/post/<id>`, `post/<id>` or the id. A post
  link of another instance (or the post's original `ap_id`) is looked up
  through yours. Posts list with links on the instance in use.
- Comments: `https://<instance>/comment/<id>`, `comment/<id>`, or the id
  `comments` lists.
- Communities: `!name@host`, `!name` (this instance), `c/name`,
  `name@host`, or a link `https://<host>/c/<name>`.
- Users: `@name@host`, `@name`, `u/name`, or a link
  `https://<host>/u/<name>`.
- `#N` works after any list.

## Commands

### hot

`--category hot|active|scaled|new|old|controversial|most-comments|new-comments|top-hour|top-day|top-week|top-month|top-year|top-all`
orders the posts of every community the instance knows (`hot` by
default).

### feed

`--type subscribed|local|all`: the communities you subscribe to (the
default when logged in, needs an account), the instance's own
communities (the default without one), or all of them; ordered by hot.

### search

Posts by default; `-t user`, `-t topic` (communities). `--filter local`
keeps the instance's own; `--sort top-all|new|old|hot|active|top-day|top-week|top-month|top-year`
(`top-all` by default).

### read

A post with its text (Markdown), link (`extra.link`), community, score,
up- and downvotes and crossposts. A comment link reads the comment's
post.

### comments, replies

The thread as Lemmy's pages load it: one request returns up to 300
comments, eight levels deep, with replies nested; top-level comments are
then paged. `--sort hot|top|new|old|controversial`. `replies POST
COMMENT` loads the thread under one comment. Deleted and removed
comments show as `[deleted]`.

### like, vote

`like POST [--undo]` upvotes a post or takes the vote back. `vote TARGET
up|down|clear` votes on a post or a comment (by link, `post/<id>` or
`comment/<id>`).

### favorite, favorites

`favorite POST [--undo]` saves a post on your account; `favorites` lists
what you saved (yours only).

### comment

`comment POST TEXT [--reply-to COMMENT]`.

### post, submit

`post TEXT --title TITLE --topic COMMUNITY` writes a text post;
`submit COMMUNITY --title TITLE [--url LINK] [--body TEXT] [--nsfw]`
also takes a link. Images cannot be uploaded; link to them with `--url`.

### edit

`edit POST [TEXT] [--title TITLE]` changes the text (Markdown), the title
or both of one of your posts. It loads the post first, prints the change
(each field as old → new) and asks (`-y` in scripts).

- What is not given stays as it was, and so do the link, the NSFW mark
  and the language. An empty TEXT with `--title` empties the text.
- Lemmy fetches a link's preview again on every edit; ATP sends the
  post's thumbnail back so a custom one stays.
- Only the author can edit a post, at any time; communities and
  comments are not edited here. The post shows as edited, and other
  instances get the new version.

```
atp lemmy edit https://lemmy.world/post/123456 'Updated: the fix is in 0.4.2'
atp lemmy edit '#1' --title 'Release notes for 0.4.2' -y
```

### delete, delete-comment

Delete one of your posts or comments, after a confirmation (`-y` in
scripts).

### notifications, unread

`notifications --type reply|mention`: replies to your posts and comments
(by default) or mentions of you, unread ones marked. `unread` counts
replies, mentions and private messages.

### user, user-posts

A user's profile (display name, `@name@host`, bio, post and comment
counts) and their posts, the newest first.

### community, community-info, communities, subscribe

`community COMMUNITY [--sort …]` lists a community's posts (same sorts as
`hot`); `community-info` shows the community; `communities [--type
all|local|subscribed] [--sort …]` lists them; `subscribe COMMUNITY
[--undo]` joins or leaves one.

### instance

The instance's name, Lemmy version, sign-up mode, whether downvotes are
on, and its counts.

## Limits

No private messages, image uploads, editing of comments, reporting, blocking or
moderation, and no two-factor setup. Threads longer than 300 comments
show only their first 300 (use `replies` for deeper branches).
