## Instances

Every command talks to one instance: the one you logged in to, else
`MISSKEY_URL` (`MISSKEY_URL=misskey.design social misskey feed`), else
misskey.io. Without an account the instance serves what it shows the public:
local and global timelines, featured notes, notes, users and hashtags (some
instances close more of it).

## References

- Notes: `https://<instance>/notes/<id>` or the id. Links to notes on other
  servers are looked up through your instance (needs an account).
- Users: `@name` (this instance), `@name@host`, a profile link
  (`https://<instance>/@name`), an id, or `me`.
- Chats: `user:<id>` or a user as above (your direct chat), `room:<id>`.
  Chat messages: `user:<id>/<message>` / `room:<id>/<message>` (the `id` of
  listed messages). `#N` works after `chats` and after any list.

## Logging in

- `social misskey login`: MiAuth. Open the printed link (or scan the code)
  signed in to the instance, allow access, and the session is saved.
  `MISSKEY_URL` picks the instance.
- `social misskey login --token 'INSTANCE|TOKEN'` with an access token from
  Settings → API (`-` reads it from stdin).
- `MISSKEY_URL` + `MISSKEY_TOKEN` supply both for one run.

## Notes

- `feed --type home|local|hybrid|global`, `hot` (featured notes),
  `search WORDS [--filter local]` (`#tag` searches a hashtag),
  `search -t user`, `search -t topic` (hashtags), `trends`, `tag TAG`,
  `mentions`.
- `read`, `comments` (replies; `--replies` follows threads), `likers` (who
  reacted, with the reaction in `extra.reaction`), `reposts` (renotes).
- `post TEXT [--title CW] [-i FILE ...] [--topic TAG] [--reply-to NOTE]
  [--quote NOTE]`; `note` takes every option (`--cw`, `--visibility
  public|home|followers|specified`, `--local-only`). `comment` replies,
  `renote [--undo]`, `react NOTE 👍` or `:emoji:` (`--undo` removes your
  reaction), `like` (❤), `favorite`, `follow`, `delete`.
- `user`, `user-posts`, `followers`, `following`, `favorites` (yours),
  `notifications [--type mention|reply|renote|quote|reaction|follow|...]`,
  `unread`, `instance`.

## Chat

Misskey 2025.4 and later: `chats` lists recent direct chats and your rooms,
`messages CHAT`, `send CHAT TEXT [-a FILE ...]` (each extra file goes as its
own message: Misskey takes one file per message), `react`, `delete`,
`members room:<id>`.

## Live

`listen` follows the home timeline plus mentions, replies, chat messages and
notifications when logged in, the local timeline without an account, or the
chats you name. `-f jsonl` prints one JSON object per event.

## Limits

No editing (Misskey has none), no clips or antennas yet, and one file per
chat message.
