## References

- Posts: the id (`1wt65ox`), `t3_<id>`, links on www / old / np.reddit.com,
  `redd.it/<id>`, `/gallery/<id>`, `v.redd.it/<id>` and share links.
- Comments: `t1_<id>` or a comment permalink.
- Subreddits: `rust`, `r/rust` or a link; users: `spez`, `u/spez` or a link.

## Logging in

`atp reddit login --browser` (the `reddit_session` cookie). Writes use the
web app's own `token_v2` bearer, renewed automatically. Alternatively set
`REDDIT_CLIENT_ID`, `REDDIT_CLIENT_SECRET`, `REDDIT_USERNAME` and
`REDDIT_PASSWORD` for a Reddit "script" app; that switches everything to
Reddit's OAuth API.

Reddit refuses anonymous API requests from many networks; log in (or use
`--proxy`) when commands answer "blocked by network security".

## Analytics

- `atp reddit insights [--days N]`: karma (post, comment, awarder,
  awardee) with per-subreddit breakdowns, followers, account age and
  trophies, plus daily numbers computed from your own posts and comments
  created in the window (their current scores, counted on their creation
  day). `extra.native` / `extra.computed` say which is which.
- `atp reddit insights <post>`: public counters (score, upvote ratio,
  comments, crossposts, awards, views when sent); for your own posts also
  the author-only post insights page.

## Interactions

- `reposts` lists crossposts; there is no `likers` (votes are private).
- `comments --all` expands every "load more" and "continue this thread".
- `sub SUBREDDIT [--sort hot|new|top|rising|controversial] [--time …]`
  browses a subreddit; `subreddit NAME` shows its details; `subreddits
  [popular|new|default]`.
- `post "text" --title T --topic r/<sub>` (or `u/<you>` for your profile):
  a single URL as text makes a link post, `-i` images an image or gallery
  post, `--quote POST` a crosspost.
- `downvote THING [--undo]`, `edit THING "text"`, `follow r/<sub>` or
  `follow u/<user>`, `block USER [--undo]`, `hide POST [--undo]`.
- `mark-read` marks the whole inbox read; `mark-read ID… [--unread]` only
  those items (`t1_…` / `t4_…` ids from `notifications`).
- `user-comments USER` lists a user's comments; `trophies [USER]`;
  `moderated [USER]` the subreddits they moderate.
- Your other lists: `saved-comments`, `hidden`, `downvoted`.
- Polls show their options, vote counts (when Reddit reveals them), total,
  end time and your choice in `extra.poll`.

## Communities

- `rules SUB`, `mods SUB` (moderators and permissions), `wiki SUB` (page
  list) and `wiki SUB PAGE` (one page as Markdown, `#N` after the list).
- `search-in SUB QUERY [--sort …] [--time …]` searches one subreddit;
  `domain github.com [--sort …]` lists posts linking a site.
- Multireddits: `multis [USER]` (yours, or a user's public ones) and
  `multi u/USER/m/NAME [--sort …]` (or `#N` after `multis`).
- Flair: `flairs SUB` lists post flairs; `flair POST [TEXT] --template ID`
  (or `--template '#N'`) sets your post's flair.

## Managing posts

- `flag THING nsfw|spoiler|replies|lock|sticky [--undo]`: NSFW, spoiler and
  inbox replies are yours to set on your own posts; lock and sticky need a
  moderator, as do `approve THING` and `remove THING [--spam]`.

## Messages and live updates

- `chats` lists your private-message threads, `messages THREAD` shows one,
  `send u/USER "text"` starts a conversation (the first line is the
  subject), `send r/SUB "text"` writes to its moderators, and `send THREAD
  "text"` (or `--reply-to MESSAGE`) answers. Reddit's newer chat (the chat
  icon) is a separate system without a public API, so it is not covered.
- `listen [r/SUB|u/USER|inbox …]` polls about every 30 s and prints new
  posts, comments or inbox items; with no argument it follows your inbox.
