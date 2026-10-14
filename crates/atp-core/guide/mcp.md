# Using ATP from AI agents (MCP)

`atp mcp` serves every platform to AI agents over the Model Context
Protocol, on standard input and output. Add it to a client:

    claude mcp add atp -- atp mcp                 # Claude Code
    claude mcp add atp -- atp mcp --read-only     # reading only

Other clients take the same command in their configuration:

    {"mcpServers": {"atp": {"command": "atp", "args": ["mcp"]}}}

## Tools

| Tool | What it does |
| --- | --- |
| `platforms` | every platform, its capabilities and its own commands |
| `help` | this guide: the index, a topic, `login [platform]`, a platform (`bili`) or a command (`bili search`, `ncm playlist create`) |
| `read` | commands that only read: search, hot, read, comments, user, followers, notifications, chats, messages, whoami ... |
| `run` | everything else: like, comment, follow, post, delete, send, download, `listen`, platform-only commands (left out with `--read-only`) |

`read` and `run` take a platform, a command and its further command-line
words, e.g. `{"platform": "bili", "command": "search", "args": ["rust", "-n", "5"]}`,
and answer with the same envelope `--json` prints. `#3` refers to the third
item of the last list, across calls.

## Resources

The guide is also served as resources, in Markdown (`text/markdown`):

| URI | Page |
| --- | --- |
| `atp://guide` | the index |
| `atp://guide/<topic>` | a topic, e.g. `atp://guide/refs`; `atp://guide/login` adds every platform's login ways |
| `atp://guide/<platform>` | a platform, e.g. `atp://guide/bili` |
| `atp://guide/<platform>/<command>` | one command; nested ones add segments: `atp://guide/ncm/playlist/create` |
| `atp://guide/login/<platform>` | logging in to one platform |

`resources/list` lists the index, the topics and the platforms;
`resources/templates/list` gives the last two forms as templates.

## Limits

- Commands that would ask a question need `--yes` in `args`.
- `wait_secs` bounds a call (default 300 s); `listen` and other live
  commands return the events seen by then (default 30 s).
- QR-code logins need a terminal: run `atp <platform> login` there once.
  `run` takes `login --cookie`, `--token` or `--browser`.
- Captchas are never opened from the server; the error carries the page in
  `error.url` for a person to open.
- The options and environment of `atp mcp` (`--proxy`, `ATP_PROXY`,
  `ATP_HOME` ...) apply to every call; `args` can add per-call options.
