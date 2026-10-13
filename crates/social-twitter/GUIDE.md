## References

- Tweets: the numeric id or a status link (`x.com/<user>/status/<id>`,
  `twitter.com/…`, with or without `https://`, `/photo/1` suffixes too).
- Users: `NASA`, `@NASA`, `https://x.com/NASA`, or the numeric id.
- Lists: the id or an `x.com/i/lists/<id>` link.
- Communities: the id or an `x.com/i/communities/<id>` link; Spaces: the id
  or an `x.com/i/spaces/<id>` link.
- Direct messages: `CONVERSATION/MESSAGE` (from `messages`); a chat is a
  conversation id, or a user for your one-to-one conversation with them.

## Logging in

X has no QR login: use `social x login --browser`, or `--cookie` with at
least `auth_token` and `ct0` (the full cookie header avoids "automated
behaviour" errors, code 226). Without a login, `user`, `read`,
`user-posts` and `download` work through a guest token.

## Analytics

- `social x insights [--days N]`: X shows account analytics (daily
  impressions, engagements, profile visits, follows, audience, top posts)
  to X Premium only. Without Premium you get a warning plus X's rollup of
  the last 8 days and numbers summed from your own posts in the window.
- `social x insights <your tweet>`: lifetime analytics without Premium
  (impressions, engagements, likes, replies, retweets, bookmarks, profile
  visits, link / media / detail clicks, new follows); daily series and
  audience need Premium. Others' tweets get public counters.

## Interactions

- `likers` works only on your own tweets (likes are private since 2024).
- `reposts` lists quote tweets; `retweeters TWEET` lists who retweeted.
- `retweet TWEET [--undo]`, `quote TWEET "text" [-i IMAGE]`.
- `comment TWEET "text" [-i IMAGE ...]` replies; `--reply-to` answers one
  of its replies.
- `post "text" [-i IMAGE ...]` (up to four images), with `--reply-to` or
  `--quote`; `delete` and `delete-comment` delete your tweets.
- `favorite` bookmarks (into a folder with `--folder`); `folders` lists
  bookmark folders for `favorites --folder`.
- `hot --category trending|for-you|news|sports|entertainment` reads trends
  (their `url` is a search, not a tweet).

Writes wait 1.5–4 s first, as X flags bursts.

## More of X

- Profile tabs: `tab USER replies|media|highlights|articles`;
  `verified-followers USER`, `followers-you-know USER`.
- Search tabs: `search --sort top|latest|media`, `search -t user`, and
  `search -t topic` for lists.
- Posts: polls (choices, votes, end) and community notes land in `extra`
  (`poll`, `community_note`), as do the community and the number of edits;
  `edits POST` lists earlier versions; `similar POST` needs X Premium.
- Lists: `list`, `list-info`, `list-members [--followers]`,
  `list-memberships [USER]`, `list-create`, `list-update`, `list-delete -y`,
  `list-add LIST USER [--undo]`, `list-follow [--undo]`.
- Communities: `community`, `community-posts --sort top|latest|media`,
  `community-members [--moderators]`, `communities` (yours), `join [--undo]`.
- Spaces: `space SPACE` (state, hosts, speakers, listeners); `spaces QUERY`
  finds posts sharing Spaces (X's own Space search is gone from the web).
- Trends: `trends --place WOEID` (with tweet volumes; `trend-places` lists the
  places, 1 is worldwide) besides `hot`.
- Account: `block` / `mute USER [--undo]`, `blocked`, `muted`,
  `pin POST [--undo]`, `scheduled`, `schedule TEXT --at +2h|'2026-10-20 09:30'`,
  `unschedule ID`, `unread`, `edit POST TEXT` (X Premium).
- Bookmark folders: `folders`, `folder-create`, `folder-rename`,
  `folder-delete -y`, `favorite POST --folder ID [--undo]`.
- Direct messages: `chats`, `messages CHAT`, `send CHAT TEXT [-a IMAGE]
  [--reply-to MSG]`, `react MSG EMOJI [--undo]`, `delete MSG`, `members CHAT`,
  `listen` (polls every ~10 s). These are X's classic DMs; the end-to-end
  encrypted chats are not served by the web API.

## Downloads

Photos at original size (`name=orig`), videos and GIFs as the best-bitrate MP4.
