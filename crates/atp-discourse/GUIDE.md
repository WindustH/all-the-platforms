Discourse forums through their JSON API. Every command talks to one
forum: `DISCOURSE_URL` when set (`DISCOURSE_URL=linux.do atp discourse
hot`), else the one you logged in to, else meta.discourse.org (Discourse's
own forum). The alias `linuxdo` is the same platform; it reaches linux.do
once you have logged in there or set `DISCOURSE_URL=linux.do`.

## Logging in

### User API Key

`atp discourse login --token FORUM_URL` (for example `https://linux.do`)
asks the forum for a User API Key, the way Discourse's own apps do: ATP
makes an RSA key pair and opens the forum's consent page
(`/user-api-key/new`) in your browser. Log in there if asked and allow
access for `ATP (all-the-platforms)`. The page then shows a block of
encrypted text: paste it into the terminal and press Enter. ATP opens it
with its private key, checks it belongs to this login, and keeps the key
with the forum and your username. The key may read, write, see your
notifications and who you are (`read,write,notifications,session_info`).
`login` needs a terminal for the paste.

Forums send the key to a redirect address only when their admins listed
it under `allowed_user_api_auth_redirects` (by default only Discourse's
own apps are listed). On such a forum, set `DISCOURSE_REDIRECT_PORT` and
ATP waits for the key at `http://127.0.0.1:PORT/callback` instead of
asking for the paste.

Who may make User API Keys is the forum's choice (by default every
user, from trust level 0); a forum that turned them off, or raised the
level, refuses on the consent page.

### Admin API key

`atp discourse login --token 'FORUM_URL|API_KEY|USERNAME'` takes a key an
admin made under Admin → Advanced → API keys, acting as `USERNAME`
(a key for all users can act as anyone). It is checked against
`/session/current.json` before it is saved.

### Environment

`DISCOURSE_URL` picks the forum for one run. `DISCOURSE_API_KEY` and
`DISCOURSE_API_USERNAME` together supply an admin API key for one run;
they win over the saved key and are never saved.

### Without an account

On an open forum everything public works: `feed` (latest), `hot`, `top`,
`categories`, `category`, `tag`, `read`, `comments`, `replies`, `search`,
`search -t user`, `user`, `user-posts`, `about`. Forums can close any of
it to visitors (a login-required forum, private categories, hidden
profiles); those answers fail with `not_authenticated`. Your new and
unread topics, notifications, unread counts, bookmarks and every write
need a key.

### Cloudflare and other checks

Some forums put a browser check in front of everything (linux.do does,
through Cloudflare). ATP asks as its browser-like client first and, when
the check stops that, once more under its own declared User-Agent, which
some checks let through (that forum is then asked that way first for a
day). When both are stopped, commands fail with `verification_required`
and the forum's address in `error.url`. ATP does not open the check in a
browser: a passed check stays with that browser and does not reach ATP,
whose requests carry no cookies. Whether a forum lets
requests with a User API Key through its check is the forum's setting
and not known to ATP.

### How long a login lasts

A User API Key lasts until you revoke it (your preferences → Apps) or
the forum revokes it, for instance after a time unused (a forum
setting). A revoked key fails with `not_authenticated`. Forums limit
requests per IP, per key and per kind of write; by default a User API
Key may make 20 requests a minute, so long listings (`-n 200`) can run
into it. Beyond a limit commands fail with `rate_limited` and say how
long to wait. Writes wait 0.5 to 1.5 seconds first.

## References

- Topics: the id, or a link `/t/<slug>/<id>`; `/t/<slug>/<id>/<n>` is
  post number `n` in it. `#N` works after any list.
- Posts: `/p/<post id>` or `post:<id>`; where a reply is expected
  (`replies`, `delete-comment`, `comment --reply-to`), the ids `comments`
  lists.
- Users: `@name`, `name`, a link `/u/<name>`, or `me` (logged in).
- Categories: the id, the slug, a link `/c/<slug>/<id>`, or `#N` after
  `categories`.

## Commands

### feed

`--type latest|new|unread|unseen`: the latest topics (the default), or
your new, unread and unseen ones (need a key).

### hot, top

`hot` is the forum's hot list (Discourse 3.2 and later). `top --period
daily|weekly|monthly|quarterly|yearly|all` lists the most active topics
of a period (weekly by default).

### categories, category, tag

`categories` lists the forum's categories with their topic counts;
`category CATEGORY` and `tag TAG` list the topics in one.

### read

A topic with its first post as the text, its views, likes and replies,
tags and category; a link with a post number reads that post.

### comments

The posts after the first, twenty a page, oldest first; `reply_to` names
the user a post answers and `extra.post_number` its number.

### replies

`replies TOPIC POST` lists the direct replies to one post.

### search

`search WORDS [--sort relevance|latest|likes|views]`; the words take the
forum's search syntax (`in:title`, `#category`, `@user`,
`status:solved` …). Each hit links to its post inside its topic. `search
-t user` searches usernames.

### user, user-posts

`user` shows a profile with its summary (posts, topics, likes given and
received, days visited); `user-posts` lists the topics and replies the
user wrote, newest first.

### favorites

Your bookmarks.

### like, favorite

`like` likes a post (a topic means its first post), `favorite` bookmarks
it; `--undo` takes either back.

### post

`post TEXT --title TITLE [--topic CATEGORY] [--topic '#tag' ...] [-i
IMAGE ...]` starts a topic: the first topic without `#` is the category
(id, slug or link), the ones with `#` are tags, images are uploaded and
added after the text. Forums set the shortest title and post they take.
With `--reply-to TOPIC` it replies in that topic instead.

### comment

Replies in the topic; `--reply-to POST` (an id `comments` lists) answers
that post; `-i` uploads images.

### edit

`edit POST [TEXT] [--title TITLE] [--topic '#tag' ...] [--topic
CATEGORY] [--no-topics]` changes one of your posts: a topic id or link
means its first post, `post:<id>` or a link with a post number another
one. It loads the post first, prints the change (each field as old →
new) and asks (`-y` in scripts).

- TEXT replaces the post's Markdown. The text ATP loaded goes along, so
  an edit made meanwhile fails with a conflict (`upstream_error`, HTTP
  409) instead of being overwritten; look again and repeat the edit.
- On a topic's first post, `--title` renames the topic, `--topic '#tag'`
  (repeatable) replaces its tags and `--no-topics` removes them, and a
  `--topic` without `#` moves it to that category (id, slug or link), as
  with `post`. Other posts take only TEXT.
- Forums limit how long a post stays editable: a day for new users and
  30 days from trust level 2 by default (each forum sets its own), and
  posts in locked or archived topics cannot change. When the forum says
  the post cannot change, ATP stops before asking with
  `permission_denied`; past the limit Discourse answers that the post is
  too old to edit. Forums also set the shortest post and title they take.
- Edits show in the post's edit history (Discourse folds small ones made
  within minutes of the last into it).

```
atp discourse edit https://meta.discourse.org/t/some-topic/123456 'New text of the first post'
atp discourse edit post:1789012 - -y < fixed.md
atp discourse edit 123456 --title 'Better title' --topic '#rust' --topic '#cli'
```

### delete, delete-comment

Deletes one of your posts; deleting a topic's first post (or the topic)
deletes the topic.

### notifications

`--type mention|reply|quote|like|message|post|linked|badge` filters the
page shown.

### about

The forum's title, description, version, topic, post and user counts,
and its admins and moderators.

## Limits

No private messages, chat, polls, or watching topics yet.
Notification filtering happens after each page arrives, so a filtered
page may hold fewer items.
