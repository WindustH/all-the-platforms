Every command talks to one instance: `MISSKEY_URL` when set
(`MISSKEY_URL=misskey.design atp misskey feed`), else the one you logged in
to, else misskey.io.

## Logging in

### MiAuth

`atp misskey login` starts MiAuth: open the printed link (or scan the
code) signed in to the instance, allow access for `all-the-platforms`, and
the token is saved with the instance. `MISSKEY_URL` picks the instance.
MiAuth asks for read and write access to the account, blocks, drive,
favorites, following, messaging, mutes, notes, notifications, reactions,
votes and chat.

### Access token

`atp misskey login --token 'INSTANCE|TOKEN'` takes an access token from
Settings → API on the instance: `INSTANCE` is its host or URL
(`misskey.io`), `TOKEN` the token (`-` reads the value from stdin). A bare
`TOKEN` is for the instance in use. The token is checked against the
instance before it is saved. Give it the permissions the commands you use
need; a missing one fails with `permission_denied` ("the token may lack
this permission").

### Environment

`MISSKEY_URL` (a host or URL) and `MISSKEY_TOKEN` supply the instance and
token for one run; they win over the saved session and are never saved.

### Without an account

Requests go out anonymously, and the instance serves what it shows the
public: the local and global timelines, featured notes, notes, users and
hashtags (some instances close more of it). Links to notes on other
servers, the home timeline, mentions, favorites, notifications, unread
counters, chat and every write need an account; without one they fail with
`not_authenticated`.

### When the token stops working

How long a token lasts is not known. When the instance refuses it,
commands fail with `not_authenticated` and the hint to log in again.
Writes wait 0.3 to 0.9 seconds first, and requests are spaced at least
half a second apart; the instance's rate limit shows up as
`rate_limited`.

## References

- Notes: `https://<instance>/notes/<id>` or the id. Links to notes on other
  servers are looked up through your instance (needs an account).
- Users: `@name` (this instance), `@name@host`, a profile link
  (`https://<instance>/@name`), an id, or `me`.
- Chats: `user:<id>` or a user as above (your direct chat), `room:<id>`.
  Chat messages: `user:<id>/<message>` / `room:<id>/<message>` (the `id` of
  listed messages). `#N` works after `chats` and after any list.

## Commands

### feed

`--type home|local|hybrid|global`; `home` (the default when logged in)
needs an account, `local` is the default without one.

### hot

Featured notes.

### search

`search WORDS [--filter local]` (`local`: notes of this instance only);
`#tag` alone lists the notes with that hashtag. `search -t user`,
`search -t topic` (hashtags).

### trends, tag

`trends` lists the hashtags in use now; `tag TAG` the notes with one.

### mentions

Notes that mention you (needs an account).

### read

A note, or a chat message given as `<chat>/<message>`.

### comments

Replies; `--replies` follows threads.

### likers, reposts

`likers` lists who reacted, with the reaction in `extra.reaction`;
`reposts` lists renotes.

### post

`post TEXT [--title CW] [-i FILE ...] [--topic TAG] [--reply-to NOTE]
[--quote NOTE]`: the title becomes the content warning, each topic a
hashtag at the end, and files go to your drive first.

### note

Takes every option: `--cw`, `--visibility
public|home|followers|specified`, `--local-only`, `--reply-to`, `--quote`,
`-i FILE`.

### comment

Writes a reply note.

### renote

`renote NOTE [--undo]` boosts a note or takes the renote back.

### react

`react NOTE 👍` or `:emoji:`; `--undo` removes your reaction. A chat
message (`<chat>/<message>`) takes reactions too.

### like, favorite

`like` reacts with ❤; `favorite` saves the note to your favorites.

### favorites

Yours only: Misskey shows nobody else's, and there are no folders (clips
are not supported yet).

### notifications

`--type mention|reply|renote|quote|reaction|follow|receiveFollowRequest|pollEnded|achievementEarned`.

### edit

`edit NOTE TEXT` replaces the text of one of your notes, on the servers
that can edit notes: Sharkey (`notes/edit`) and CherryPick
(`notes/update`, for the roles its admins allow). Misskey itself has no
edits (2023.9.2 brought them, 2023.10.0 took them out), and other
servers are refused before anything is sent; ATP tells them apart by the
server's nodeinfo. It loads the note first, prints the change (the text
as old → new, with what it keeps) and asks (`-y` in scripts).

- Only the text changes. The content warning, files, poll (with its
  votes), quote, reply, channel and visibility go along as they are;
  on CherryPick also an event, the right-click setting and a scheduled
  deletion.
- Notes with a closed poll (and on CherryPick an ended event) are
  refused: the server would not take them back.
- Chat messages cannot be edited here.

```
atp misskey edit 9xyzabc123 'Fixed the link: https://example.com'
```

### delete

A note of yours, or a chat message given as `<chat>/<message>`.

### instance

The instance's name, version, longest note, whether sign-ups are open, and
its user, note and known-instance counts.

### chats, messages, send, members

Misskey 2025.4 and later: `chats` lists recent direct chats and your rooms,
`messages CHAT`, `send CHAT TEXT [-a FILE ...]` (each extra file goes as its
own message: Misskey takes one file per message; there is no
`--reply-to`), `react`, `delete`, `members room:<id>` (for a direct chat,
the two of you).

### listen

Follows the home timeline plus mentions, replies, chat messages and
notifications when logged in, the local timeline without an account, or the
chats you name. `-f jsonl` prints one JSON object per event. A stream that
keeps closing ends with an error.

## Limits

No editing on Misskey itself (only on Sharkey and CherryPick), no clips or antennas yet, and one file per
chat message.
