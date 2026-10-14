Threads through Meta's official Threads API (graph.threads.net). Every
command needs an access token of your own Meta app; there is no
anonymous reading.

## Logging in

### Your Meta app

1. At https://developers.facebook.com/apps create an app and add the
   **Access the Threads API** use case.
2. Under the use case's permissions, add what the commands you want
   need (below). `threads_basic` is always required.
3. While the app is in development, only accounts added under **App roles
   → Roles → Threads Testers** (and accepted in the Threads app, Settings
   → Account → Website permissions → Invites) can use it. Other accounts,
   and permissions with "advanced access", need Meta's app review.
4. Get a token for your account: the use case's **Settings → User Token
   Generator** gives one for a tester, and your own OAuth flow gives a
   short-lived one (an hour), which ATP exchanges for a long-lived one
   (below).

| Permission | Unlocks |
| --- | --- |
| `threads_basic` | `whoami`, `user me`, `user-posts me`, `read`, `download` |
| `threads_content_publish` | `post`, `thread`, `comment`, `repost`, `quota` |
| `threads_read_replies` | `comments`, `replies` |
| `threads_manage_replies` | `hide` |
| `threads_manage_insights` | `insights` |
| `threads_manage_mentions` | `notifications` |
| `threads_keyword_search` | `search` |
| `threads_delete` | `delete`, `delete-comment` |
| `threads_profile_discovery` | `user` and `user-posts` of other accounts |

### Access token

`atp threads login --token ACCESS_TOKEN` checks a long-lived token
against `/me` and saves it. `atp threads login --token
'APP_SECRET|SHORT_LIVED_TOKEN'` first exchanges a short-lived token
(`th_exchange_token`) for a long-lived one with the app secret (App
settings → Basic). `-` reads the value from stdin.

Meta accepts only HTTPS redirect addresses for the Threads login, so ATP
offers no browser login through a local address: get the token from the
User Token Generator or your own HTTPS redirect.

### How long a token lasts

A long-lived token is valid for 60 days. ATP renews a saved token
(`th_refresh_token`) once it is a week old, on the next command, and
`atp threads refresh` renews it at once; a token has to be at least a
day old to be renewed. When the token has expired or was revoked,
commands fail with `not_authenticated` and the hint to log in with a new
one.

### Environment

`THREADS_ACCESS_TOKEN` supplies a token for one run; it wins over the
saved one, is never saved and never renewed by ATP.

### Limits Meta sets

A missing permission fails with `permission_denied` and names the
permission. Meta allows 250 posts and 1000 replies per account in 24
hours (`quota` shows the use); its rate limits show as `rate_limited`.

## References

- Posts: the id (digits), or a link
  `https://www.threads.com/@user/post/CODE` (threads.net links too).
  The API looks posts up by id only: ATP remembers the links of the posts
  it lists, and looks for other links among the author's latest 500
  posts. `#N` works after any list.
- Users: `me`, `@username`, `username`, or a profile link.

## Commands

### whoami, user

`user me` is your profile. Other accounts (`user @name`) come from
`profile_lookup`, with followers, likes, views, replies, reposts and
quotes counts; that needs `threads_profile_discovery`.

### user-posts

`user-posts me` lists your threads. Another account's public posts need
`threads_profile_discovery`.

### read, download

A post with its text, link, topic tag, quote and repost ids, and its
image, video or carousel items. `download` saves the media.

### comments, replies

`comments POST` lists the whole conversation under a post (replies at
every depth, oldest first; `reply_to` is the id each one answers);
`replies POST REPLY` the direct replies to one reply.

### post

`post TEXT [--reply-to POST] [--quote POST] [--topic TAG]` publishes a
text post (500 characters at most; the first topic becomes the topic
tag). Threads takes media only by public URL, so `-i FILE` is not
supported: use `thread`.

### thread

Every publishing option: `--image URL` and `--video URL` (repeatable;
two to twenty items make a carousel), `--reply-to`, `--quote`, `--topic`,
`--link URL` (a link preview, text posts only), `--reply-control
everyone|accounts_you_follow|mentioned_only|parent_post_author_only|followers_only`.
Meta fetches each URL itself (JPEG or PNG images, MP4 or MOV videos), and
ATP waits until the containers are processed (five minutes at most)
before publishing.

### comment

Publishes a reply to the post (or to `--reply-to REPLY`); no images.

### delete, delete-comment

Deletes one of your posts or replies (`threads_delete`).

### repost

`repost POST` reposts a post.

### hide

`hide REPLY [--undo]` hides a reply to one of your posts, or shows it
again.

### notifications

Posts that mention you (`threads_manage_mentions`); `--type` has no
effect.

### search

`search WORDS [--sort top|recent]`; `#tag` alone searches a topic tag.
It needs `threads_keyword_search`; without that permission it fails with
`permission_denied`. Meta documents that until the app has advanced
access to it, searches return only posts of the authenticated account.

### insights

`insights [--days N]` (90 at most): views by day, likes, replies,
reposts, quotes, link clicks, followers, and followers by country, city,
age and gender (accounts with at least 100 followers). `insights POST`:
lifetime views, likes, replies, reposts, quotes and shares of one of
your posts. A metric Meta refuses becomes a warning.

### quota

How many posts and replies of today's allowance are used.

### refresh

Renews the saved token now (not one from `THREADS_ACCESS_TOKEN`).

## Limits

Nothing works without a token, and there is no home feed, trending list,
like or follow in the API. Media are published from public URLs only.
Links of posts ATP has not listed are found among the author's latest
500 posts at most. Other accounts' profiles and posts need
`threads_profile_discovery`, which Meta grants after review.
