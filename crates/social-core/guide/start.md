# Getting started

`social` drives six platforms through one command set:

    social <platform> <command> [arguments] [options]

Platforms (each also answers to shorter names):

| Platform | Name | Aliases |
| --- | --- | --- |
| Zhihu | `zhihu` | `zh` |
| Xiaohongshu / RedNote | `xhs` | `xiaohongshu`, `rednote` |
| Twitter / X | `twitter` | `x`, `tw` |
| Bilibili | `bili` | `bilibili`, `b23` |
| Reddit | `reddit` | `rd` |
| YouTube | `youtube` | `yt` |

`social platforms` shows which commands each platform supports.

## First steps

Many reads work without an account:

    social bili hot -n 5                 # trending videos
    social bili read '#1'                # the first item of the last list
    social bili comments '#1' -n 20      # its comments
    social yt search "rust tutorial" -n 5
    social x user-posts NASA -n 10

Log in for everything else (see `social guide login`):

    social bili login                    # scan a QR code with the app
    social x login --browser             # reuse a logged-in browser
    social zhihu status                  # check a saved session

Write, analyse, download:

    social bili like BV1xx411c7mD
    social bili insights --days 30       # creator analytics of your account
    social bili download BV1xx411c7mD --audio-only

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
Zhihu questions, YouTube transcripts ...) are listed in `social guide <platform>`.

## Getting help

    social --help                        # global options and platforms
    social <platform> --help             # a platform's commands
    social <platform> <command> --help   # options, with examples
    social guide                         # all guide topics
    social guide <topic>                 # one topic, e.g. `social guide analysis`

Guide topics print as plain Markdown when stdout is not a terminal, so
scripts and AI agents can read them too.
