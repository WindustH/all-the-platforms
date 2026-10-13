# Getting started

`atp` drives social networks, video sites and chat apps through one
command set:

    atp <platform> <command> [arguments] [options]

`atp platforms` lists every platform with its shorter names and the
commands it supports; `atp guide <platform>` explains one of them.

- Social and content: `zhihu`, `xhs`, `weibo`, `tieba`, `mp` (WeChat
  Official Accounts), `twitter`, `reddit`, `facebook`, `instagram`, `misskey`.
- Video: `bili`, `douyin`, `youtube`, `tiktok`.
- Chat: `wechat`, `telegram`, `discord`, `whatsapp`, `qq`, `onebot`,
  `feishu`, `dingtalk`, `wecom`, `slack`, `mattermost`, `rocketchat`,
  `matrix`, `kook`, `vocechat`, `line`, `satori`.

Chat apps share `chats`, `messages`, `send`, `members`, `react`, `edit` and
`listen` (see `atp guide interact`).

## First steps

Many reads work without an account:

    atp bili hot -n 5                 # trending videos
    atp bili read '#1'                # the first item of the last list
    atp bili comments '#1' -n 20      # its comments
    atp yt search "rust tutorial" -n 5
    atp x user-posts NASA -n 10

Log in for everything else (see `atp guide login`):

    atp bili login                    # scan a QR code with the app
    atp x login --browser             # reuse a logged-in browser
    atp zhihu status                  # check a saved session

Write, analyse, download:

    atp bili like BV1xx411c7mD
    atp bili insights --days 30       # creator analytics of your account
    atp bili download BV1xx411c7mD --audio-only

## The shared commands

| Group | Commands |
| --- | --- |
| Account | `login`, `logout`, `status`, `whoami` |
| Discover | `search`, `hot`, `feed` |
| Content | `read` (`show`), `comments`, `replies`, `likers`, `reposts`, `download` |
| People | `user`, `user-posts` (`posts`), `followers`, `following` |
| Your library | `collections`, `favorites` (`bookmarks`), `likes`, `history`, `notifications`, `unread` |
| Interact | `like` / `unlike`, `favorite` / `unfavorite`, `comment`, `delete-comment`, `follow` / `unfollow`, `post` (`publish`), `delete` |
| Analytics | `insights [POST]` |

Commands a platform does not support are hidden from its `--help` and
answer `unsupported_operation`. Platform-only commands (Bilibili coins,
Zhihu questions, YouTube transcripts ...) are listed in `atp guide <platform>`.

## Getting help

    atp --help                        # global options and platforms
    atp <platform> --help             # a platform's commands
    atp <platform> <command> --help   # options, with examples
    atp guide                         # all guide topics
    atp guide <topic>                 # one topic, e.g. `atp guide analysis`

Guide topics print as plain Markdown when stdout is not a terminal, so
scripts and AI agents can read them too.
