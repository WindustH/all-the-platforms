## Logging in

- `social mattermost login --token 'https://chat.example.com TOKEN'` with a
  personal access token (Profile → Security → Personal Access Tokens; the
  server admin may need to enable them), or
  `--token 'https://chat.example.com LOGIN PASSWORD'` for a password login
  (the session token it yields is saved; MFA and SSO accounts need a
  personal access token). Separate the parts with `|` when the password
  has spaces; `-` reads the value from stdin.
- `MATTERMOST_URL` and `MATTERMOST_TOKEN` supply both for one run.

## References

- Posts: permalinks (`https://SERVER/TEAM/pl/ID`), post ids, or
  `CHANNEL/POST`. `#N` works after a list.
- Channels: ids, links (`https://SERVER/TEAM/channels/NAME`), `TEAM/NAME`,
  `~name` / `name` (looked up in your teams), or `@user` for the direct
  channel. Teams: ids or names.
- Users: ids, `@name`, emails, `me`.

## Chats

- `chats` lists your teams and direct / group channels; `chats TEAM` the
  team's channels you belong to (most recent first).
- `messages CHANNEL [-n N] [--since 7d]`, `send CHANNEL "text" [-a FILE …]
  [--reply-to POST]` (the reply goes into the post's thread),
  `comment POST "text"`, `comments POST` (its thread), `edit`, `delete`,
  `react POST 👍` (or a name like `tada`; `--undo`).
- `members CHANNEL` or `members TEAM`.
- `listen [CHANNEL …]` streams new, edited and deleted posts and reactions.
- `search 'terms from:ana in:town-square after:2026-01-01'` searches your
  teams (`team:NAME` narrows it); `notifications` lists posts that mention
  you; `unread` counts per team.
- `favorites` lists your flagged posts; `favorite POST [--undo]` flags one.
- `download POST` saves its files.

## Other commands

`teams`, `channel CHANNEL`, `pins CHANNEL`, `pin POST [--undo]`,
`join CHANNEL`, `leave CHANNEL`.
