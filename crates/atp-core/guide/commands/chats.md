On chat platforms `chats` lists the conversations the account is in:
servers, teams, groups, channels and direct chats. `chats PARENT` lists the
channels inside one server, team or space. `members CHAT` lists who is in a
chat, group or server.

    atp discord chats                 # servers and direct chats
    atp discord chats '#2'            # the channels of the second server
    atp tg members @rustlang -n 200

- A chat is a Collection: `id`, `kind` (`server`, `channel`, `group`,
  `dm`, `thread` ...), `name`, `description`, `url`, `owner`, platform
  fields in `extra`. Its `id` is what CHAT takes in `messages`, `send`,
  `members` and `listen`.
- After `chats`, `#N` as CHAT or PARENT is the N-th chat. Chats share one
  `#N` list with other collections (topics, folders), so the latest of
  those listings counts.
- `members` gives Users, and `#N` then refers to them (`user '#1'`). Who
  may list members differs per platform; see the platform's notes.
- `-n` and `--cursor` page as everywhere; `--since` / `--until` do not
  apply.

More: `atp guide refs`
