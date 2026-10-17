## Logging in

X has no QR login. `atp x login` (the same as `--browser`) reads the
session of a browser logged in at x.com. With `--cookie`, copy the whole
`Cookie` header of a request to `x.com` (developer tools, Network tab).
It needs at least `auth_token` (the session) and `ct0` (the CSRF token
every request repeats in a header); the full cookie header avoids
"automated behaviour" errors (code 226). `twid` names the account's id;
without it ATP looks the account up.

Without a login, `user`, `read`, `user-posts`, `download` and the public
counters of a tweet (`insights POST`) work through a guest token, which
ATP fetches itself, keeps for 2 hours and renews once when X refuses it.
Everything else (search, replies, followers, trends, your timelines,
lists, communities, Spaces, direct messages, your analytics and every
write) needs the session.

How long a session lasts is not known. When X stops accepting it, commands
fail with `not_authenticated` and a hint to log in again.

X flags requests it takes for automation (code 226): the command fails
with `verification_required`; open x.com in a browser, pass any check
there and try again later. A locked account (code 326) also gives
`verification_required`; unlock it at x.com. Writes wait 1.5–4 s first,
as X flags bursts.

`ATP_TWITTER_COOKIE` supplies a cookie header for one run without saving
it.

## References

- Tweets: the numeric id or a status link (`x.com/<user>/status/<id>`,
  `twitter.com/…`, with or without `https://`, `/photo/1` suffixes too).
- Users: `NASA`, `@NASA`, `https://x.com/NASA`, or the numeric id.
- Lists: the id or an `x.com/i/lists/<id>` link.
- Communities: the id or an `x.com/i/communities/<id>` link; Spaces: the id
  or an `x.com/i/spaces/<id>` link.
- Direct messages: `CONVERSATION/MESSAGE` (from `messages`); a chat is a
  conversation id, or a user for your one-to-one conversation with them.

## Commands

### search

`--sort top|latest|media` picks the search tab;
`--filter media|images|videos|links` adds `filter:…` to the query.
`-t user` finds accounts, `-t topic` lists (X's search has no topics).

### hot

`--category trending|for-you|news|sports|entertainment` reads the trends
of an Explore tab; their `url` is a search, not a tweet. `trends` gives
the trends of a place.

### feed

`-t for-you` (the default) or `-t following` (chronological).

### read

Polls (choices, votes, end) and community notes land in `extra` (`poll`,
`community_note`), as do the community and the number of edits.

### comments, replies

`--sort relevance|recency|likes`. A comment is a reply tweet, so
`replies POST COMMENT` reads the replies under that tweet.

### likers

Works only on your own tweets (likes are private since 2024); `read`
shows the like count of others'.

### reposts, retweeters

`reposts` lists quote tweets, newest first; `retweeters TWEET` lists who
retweeted.

### tab, verified-followers, followers-you-know

Profile tabs: `tab USER replies|media|highlights|articles`;
`verified-followers USER`, `followers-you-know USER`.

### collections

The lists a user owns; yours without a USER.

### favorites, favorite, unfavorite

Favorites are bookmarks, which only you see: `favorites` takes no USER;
`--folder ID` lists one bookmark folder. `favorite` bookmarks a tweet;
`favorite POST --folder ID` files a bookmarked tweet into a folder
(`unfavorite POST --folder ID` takes it out).

### folders, folder-create, folder-edit, folder-delete, folder-add, folder-remove, folder-move

Bookmark folders (an X Premium feature). `folders` lists them: FOLDER
of the `folder-*` commands and `--folder` take their ids or `#N` after
`folders` (`collections` lists your lists, not folders).

- `folder-create NAME`; `folder-edit FOLDER --name NAME` renames (a
  folder has no description, privacy or cover).
- `folder-delete FOLDER… [-y]` asks first; the bookmarks stay
  bookmarked.
- `folder-add FOLDER POST…` files bookmarked tweets into a folder and
  `folder-remove` takes them out, one call per tweet; bookmark a tweet
  first with `favorite`.
- `folder-move FROM TO POST… [--copy]` files them into TO, then takes
  them out of FROM.

### notifications, unread

`notifications -t all|verified|mentions`. `unread` counts unread
notifications and direct messages.

### retweet, quote

`retweet TWEET [--undo]` (also `repost`), `quote TWEET "text" [-i IMAGE]`.

### comment, delete-comment

`comment TWEET "text" [-i IMAGE ...]` replies; `--reply-to` answers one of
its replies. `delete-comment` deletes your reply tweet.

### post

`post "text" [-i IMAGE ...]` (up to four images: jpeg, png, gif or webp,
5 MB each), with `--reply-to` or `--quote`. `--title` becomes the first
paragraph, each `--topic` a hashtag at the end.

### delete

Deletes your tweets, or a direct message (`CONVERSATION/MESSAGE`).

### edit, edits

`edit POST TEXT` needs X Premium and works within an hour of posting,
five times at most; the new version replaces the tweet. `edits POST`
lists the earlier versions of an edited tweet.

### similar

`similar POST` needs X Premium.

### schedule, scheduled, unschedule

`schedule TEXT --at WHEN [-i IMAGE]` takes `+30m`, `+2h`, `+1d`, a local
`'2026-10-20 09:30'` or an RFC 3339 time, in the future. `scheduled`
lists your scheduled posts, soonest first; `unschedule ID` cancels one.

### pin, mute, muted

`pin POST [--undo]`, `mute USER [--undo]`; `muted` lists the accounts
you mute.

### block, unblock, blocked

`block USER…` / `unblock USER…` one call per user; `blocked` lists the
accounts you block.

### list, list-info, list-members, list-memberships

`list LIST` reads the latest tweets of a list, `list-info` its details,
`list-members [--followers]` its members (or followers),
`list-memberships [USER]` the lists a user (you, by default) is in.

### list-create, list-update, list-delete, list-add, list-follow

Your lists:

- `list-create NAME [-d TEXT] [--private]`;
- `list-update LIST [--name] [-d] [--private true|false]`;
- `list-delete LIST [-y]` (asks first; `-y` in scripts);
- `list-add LIST USER [--undo]`, `list-follow LIST [--undo]`.

### community, community-posts, community-members, communities, join

`community`, `community-posts --sort top|latest|media`,
`community-members [--moderators]`, `communities` (yours),
`join [--undo]`.

### space, spaces

`space SPACE` (state, hosts, speakers, listeners); `spaces QUERY` finds
posts sharing Spaces, newest first, with the Space id in `extra.space`
(X's own Space search is gone from the web).

### trends, trend-places

`trends --place WOEID` (with tweet volumes; `trend-places` lists the
places, 1 is worldwide) besides `hot`.

### chats, messages, members

X's classic direct messages: `chats` lists your conversations,
`messages CHAT` reads one, `members CHAT` lists who is in it. The
end-to-end encrypted chats are not served by the web API.

### send

`send CHAT TEXT [-a IMAGE] [--reply-to MSG]`. A message carries one
attachment: the first image goes with the text, the others follow as
messages of their own.

### react

`react MSG EMOJI [--undo]`.

### listen

Polls the inbox every 10–15 s.

### insights

- `atp x insights [--days N]`: X shows account analytics (daily
  impressions, engagements, profile visits, follows, audience, top posts)
  to X Premium only. Without Premium you get a warning plus X's rollup of
  the last 8 days and numbers summed from your own posts in the window.
- `atp x insights <your tweet>`: lifetime analytics without Premium
  (impressions, engagements, likes, replies, retweets, bookmarks, profile
  visits, link / media / detail clicks, new follows); daily series and
  audience need Premium. Others' tweets get public counters.

### download

Photos at original size (`name=orig`), videos and GIFs as the
best-bitrate MP4. No login needed.

## Limits

- Requests are at least 1.5 s apart; writes wait another 1.5–4 s.
- X's rate limits (codes 88, 185, 344, 348, 349) end a command with
  `rate_limited`.
- ATP speaks X's web GraphQL API. It reads the current query ids from
  X's web app scripts (completed by the community `placeholder.json` of
  fa0311/twitter-openapi on GitHub) and refreshes them once when X answers
  404; a 404 that stays means the id is outdated (try later) or the
  operation needs a login.
