# Interacting: likes, comments, follows, posts

All of these need a login and act as your account.

    atp <platform> like POST [--undo]          # unlike: `unlike POST`
    atp <platform> favorite POST [--folder F] [--undo]
    atp <platform> comment POST "text" [--reply-to COMMENT] [-i IMAGE ...]
    atp <platform> delete-comment POST COMMENT [-y]
    atp <platform> follow USER [--undo]        # unfollow: `unfollow USER`
    atp <platform> post "text" [--title T] [-i IMAGE ...] [--topic T ...]
                         [--reply-to POST] [--quote POST]
                         [--video FILE | --article FILE] [--cover IMAGE]
                         [--visibility V] [--ai] [--schedule WHEN]
                         [--collection C] [--music ID] [-y]
    atp <platform> delete POST [-y]

- `post -` and `comment POST -` read the text from stdin:
  `cat draft.md | atp zhihu post -`.
- `comment -i` attaches images where the platform takes them (Twitter / X;
  `comment-images` in `atp platforms`); the text may then be left out.
- `delete` and `delete-comment` ask for confirmation on a terminal; scripts
  pass `-y`. So does `post` on Douyin, before it uploads anything.
- `--video`, `--article`, `--cover`, `--visibility`, `--ai`, `--schedule`,
  `--collection` and `--music` exist where the platform offers them
  (`post-video`, `post-article`, `post-ai-label` ... in `atp platforms`).
- `follow '#N'` takes from the last user list. Where following joins a
  chat, group or community it takes from that list instead (Telegram,
  WhatsApp and Matrix chats), or from whichever came last where it takes
  both (Reddit users or subreddits, Spotify users or playlists).
- The result is an Action with the id and link of anything created.
- Writes pause a moment first on platforms that flag bursts.

What `post` publishes, per platform:

| Platform | `post` creates | Required | `--topic` | Others |
| --- | --- | --- | --- | --- |
| Zhihu | a pin (想法) | text or image | — | `ask`, `article` commands |
| Xiaohongshu | an image note | `--title`, `-i` | hashtags | `#tags` in the text too |
| Twitter / X | a tweet (4 images max) | text or image | hashtags | `--reply-to`, `--quote` |
| Bilibili | a dynamic | text or image | a topic id | `--quote` reposts |
| Reddit | a text, link, image or gallery post | `--title`, `--topic r/sub` or `u/you` | subreddit | a single URL makes a link post; `--quote` crossposts |
| Douyin | a video, an image post (图文) or an article (文章) | `--video`, `-i` or `--article` | hashtags | `--ai`, `--visibility`, `--schedule`, `--collection`, `--music`; asks first |
| YouTube | — | | | uploads are not supported |

Reply-to forms: Bilibili takes `ROOT` or `ROOT:PARENT`; Reddit and X take the
comment's id or link. Platform-only interactions (Bilibili `coin`,
`triple`; X `retweet`, `quote`; Reddit `downvote`, `edit`; YouTube
`dislike`; Zhihu `follow-question`) are in each platform's guide.

Be gentle: platforms restrict accounts that behave like bots. Space out
bulk actions (`--interval`), and do not like or follow in loops.
