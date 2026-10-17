Bluesky through the AT Protocol's XRPC API. Without an account, reads go
to Bluesky's public AppView (public.api.bsky.app); after logging in,
everything goes to your PDS (the server that keeps your account, found
from your handle), which answers with your view of each post.

## Logging in

### App password

`atp bluesky login --token 'HANDLE|APP_PASSWORD'` logs in with an app
password made under Settings → Privacy and security → App passwords
(`HANDLE` may also be your DID or email). ATP resolves the handle to its
DID and the DID document to your PDS (a self-hosted PDS works the same
way), creates a session there (`com.atproto.server.createSession`) and
keeps its access and refresh tokens. `HANDLE|APP_PASSWORD|PDS_URL` names
the PDS outright. Pipe the value to keep the password out of the shell
history:

    printf '%s' 'alice.bsky.social|xxxx-xxxx-xxxx-xxxx' | atp bluesky login --token -

Use an app password, not the account password: app passwords skip the
email code an account with two-factor login asks for (the login then
fails with `AuthFactorTokenRequired`), and can be revoked one by one.
App passwords cannot read or send chat messages unless made with that
option, and ATP has no chat commands.

### How long a session lasts

The access token lasts about two hours; ATP renews it with the refresh
token before it runs out and saves the new pair at once. How long a
refresh token lasts is set by the PDS and not known to ATP; when it has
run out, or the app password was revoked, commands fail with
`not_authenticated` and the hint to log in again. Bluesky limits
logins (createSession) per account per day, so log in once and let ATP
renew the session.

### Without an account

The public AppView answers `hot` (the Discover feed), `feed --type
discover`, `feedgen`, `trends`, `read`, `comments`, `replies`, `likers`,
`reposts`, `reposted-by`, `user`, `user-posts`, `followers`,
`following`, `follow-group` (anyone's list) and `search -t user`. It refuses post search
(`not_authenticated`). Accounts that asked to be hidden from logged-out
visitors are still served by the API; respect their wish. Your timeline,
notifications, unread counts, likes, bookmarks, your lists, blocks and
mutes and every write need an account.

### Risk control

Bluesky limits requests per account and per IP; beyond them commands
fail with `rate_limited`. Writes wait 0.3 to 0.9 seconds first and
requests are spaced at least 0.3 seconds apart.

## References

- Posts: an `at://<did>/app.bsky.feed.post/<rkey>` URI, or a link
  `https://bsky.app/profile/<handle or did>/post/<rkey>` (handles are
  resolved to DIDs and cached for a day). `#N` works after any list.
- Accounts: `@handle`, `handle`, a DID, a profile link, or `me`.
- Feeds: an `at://<did>/app.bsky.feed.generator/<rkey>` URI, a link
  `https://bsky.app/profile/<handle>/feed/<rkey>`, or `#N` after `trends`.
- Lists: an `at://<did>/app.bsky.graph.list/<rkey>` URI, a link
  `https://bsky.app/profile/<handle>/lists/<rkey>`, or `#N` after
  `follow-groups`.

## Commands

### hot

Bluesky's Discover feed.

### feed

`--type following|discover`: your timeline (the default when logged in)
or Discover.

### feedgen

`feedgen FEED` lists the posts of any custom feed. After `trends`,
`feedgen '#N'` lists the posts of a trending topic.

### trends

Topics trending now, each with the feed of its posts.

### search

`search WORDS [--sort top|latest]` searches posts (needs an account; the
words take Bluesky's search syntax: `from:handle`, `#tag`, `lang:en` …).
`search -t user` searches accounts and works without one.

### read, download

A post with its text, images, video, link card, quoted post and
counters (`favorites` is how often it was bookmarked). `download` saves
images at full size and fetches a video whole from its author's PDS
instead of the streaming playlist `read` shows.

### comments, replies

`comments POST` lists the replies as threads, ten levels deep, with the
replies of each reply inside it; `replies POST REPLY` does the same from
one reply down.

### likers, reposts, reposted-by

`likers` lists who liked a post, `reposts` the posts quoting it, and
`reposted-by` who reposted it.

### favorites, likes

`favorites` lists your bookmarks, `likes` the posts you liked; Bluesky
shows only your own.

### like, favorite, repost

`like` likes a post, `favorite` bookmarks it, `repost POST` reposts it;
`--undo` takes each back.

### post, comment

`post TEXT [-i IMAGE ...] [--topic TAG] [--reply-to POST] [--quote
POST]`: links, `@handle.domain` mentions and `#hashtags` in the text
become rich-text facets (mentions of handles that do not resolve stay
plain text); each topic becomes a hashtag at the end. Up to four images,
each under 1 MB (ATP does not shrink them); `--quote` with images makes
a quote with media. Posts take 300 characters (graphemes) and have no
title. `comment` replies to a post (or to `--reply-to REPLY`) in its
thread, with images too.

### follow

`follow USER [--undo]`.

### block, unblock, blocked

`block USER…` writes a block record for each (blocks are public on
Bluesky); a user already blocked is left as is. `unblock USER…` deletes
the block record your view of the profile names. `blocked` lists the
accounts you block.

### mute, muted

`mute USER… [--undo]` mutes accounts (private: only you know) or unmutes
them; `muted` lists the accounts you mute fully.

### follow-groups, follow-group, follow-group-create, follow-group-rename, follow-group-delete, follow-group-add, follow-group-remove

Your curation lists (user lists, not moderation lists) are the follow
groups. A list may hold accounts you do not follow, and anyone can see
it.

- `follow-groups` lists yours, `follow-group LIST` the accounts on any
  list, each with its entry record in `extra.list_item`.
- `follow-group-create NAME` makes a list (64 bytes of name at most);
  `follow-group-rename LIST NAME` puts its record back with the new name,
  keeping the description and picture.
- `follow-group-delete LIST [-y]` asks first, then deletes the list with
  its entries, ten records per `applyWrites` call.
- `follow-group-add LIST USER…` writes an entry per account (one already
  on the list is left); `follow-group-remove LIST USER…` deletes their
  entries. Both read the list's entries first.

### notifications

`--type like|repost|follow|mention|reply|quote|starterpack-joined`.

### delete, delete-comment

Deletes one of your posts or replies.

## Limits

No chat (direct messages), moderation lists, starter packs, video
uploads, alt text or languages on new posts, and no link cards for links
in new posts.
