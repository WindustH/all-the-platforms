<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/social-cli-logo-dark.svg">
    <img src="docs/assets/social-cli-logo-light.svg" alt="social-cli" width="360">
  </picture>
</p>

<p align="center">
  <strong>One command line for 31 social, video and chat platforms.</strong>
</p>

<p align="center">
  <a href="docs/output.md">Output format</a> ·
  <a href="docs/architecture.md">Architecture</a> ·
  <a href="README.zh-CN.md">简体中文</a>
</p>

---

`social` reads, searches, publishes, chats and downloads on Zhihu,
Xiaohongshu, Weibo, Bilibili, Douyin, WeChat, Telegram, Discord, YouTube,
TikTok, Reddit, Twitter / X and many more, with the same commands
everywhere. It works in a terminal for people, and in scripts and AI agents
through a stable JSON / YAML output.

```console
$ social bili search "rust 教程" -n 3
 #  Content                                                      Author        Likes  Cmts  Views   Time
 1  Rust编程语言入门教程（Rust语言/Rust权威指南配套）【已完结】  软件工艺师    26.1k  2.6k    1.7M  2020-10-21
 2  【Rust腐蚀 新手教程】2026保姆级教程 实时更新！！！           Utaoki丶苦药   1.9k   401  155.3k  2025-01-13
 3  Rust语言 Slint教程，非tauri                                  猩猩程序员        0     0    7.5k

$ social bili read '#1'
$ social tg listen -f jsonl          # every new Telegram message, as it arrives
```

## Why social-cli

- **Learn it once.** `search`, `hot`, `feed`, `read`, `comments`, `user`,
  `like`, `comment`, `follow`, `post`, `download` ... mean the same thing on
  every platform, and chat apps share `chats`, `messages`, `send`, `react`
  and `listen`. `social platforms` shows what each one supports.
- **Made for agents and scripts.** Every result comes in one envelope with
  stable fields and error codes; piped output switches to YAML by itself.
  Posts, messages, users and comments share one shape across platforms.
- **Ready for analysis.** Creator analytics (`insights`: trends, traffic
  sources, audience), who liked and reposted a post, complete comment
  threads, chat exports, `--since` / `--until`, and CSV or JSON Lines output
  that drops straight into pandas or DuckDB.
- **Live events.** `listen` streams new messages from chats, groups and
  channels as they happen, one line or one JSON object per event.
- **Short references.** After any list, `#3` means "the third item": no
  copying of long ids or links.
- **Paging that never loses its place.** `-n 200` follows pages for you;
  `--cursor` continues exactly where the last run stopped.
- **Log in the easy way.** Scan a QR code with the app, reuse your browser's
  session, or give a bot token or API key. Sessions stay on your machine.
- **Much works without an account.** Public videos, posts, comments, users,
  channels and articles on most platforms.
- **Downloads included.** Original-quality images, videos with their audio
  merged, audio only, or speech-recognition-ready WAV segments.
- **Behaves like a browser.** Browser network fingerprints, each platform's
  request signatures and polite pacing keep requests looking ordinary.

## Platforms

| | Platform | Command | Highlights | Sign in |
| --- | --- | --- | --- | --- |
| Social | Zhihu 知乎 | `zhihu` | answers, articles, pins, questions, creator analytics | QR, browser |
| | Xiaohongshu 小红书 | `xhs` | notes, comments, activity center, creator data | QR, browser |
| | Weibo 微博 | `weibo` | posts, hot search, comments, reposts, analytics | QR, browser |
| | Tieba 百度贴吧 | `tieba` | forums, threads, floors, check-in | QR, browser |
| | WeChat Official Accounts 公众号 | `mp` | articles, albums, drafts, publishing, comments, analytics | QR, AppID |
| | Twitter / X | `twitter` | tweets, threads, lists, analytics | browser |
| | Reddit | `reddit` | posts, comments, subreddits, inbox | browser, app |
| | Facebook | `facebook` | profiles, pages, groups, Page insights, ads | browser, token |
| | Instagram | `instagram` | posts, reels, stories, DMs, insights, ads | browser, token |
| | Misskey | `misskey` | notes, timelines, reactions, chat | MiAuth, token |
| Video | Bilibili 哔哩哔哩 | `bili` | videos, danmaku, subtitles, coins, creator data | QR, browser |
| | Douyin 抖音 | `douyin` | videos, image posts, hot boards, no-watermark downloads | browser |
| | YouTube | `youtube` | videos, Shorts, transcripts, playlists, analytics | browser |
| | TikTok | `tiktok` | videos, sounds, hashtags, ads | QR, browser |
| Chat | WeChat 微信 (ClawBot) | `wechat` | receive and answer messages | QR |
| | Telegram | `telegram` | chats, channels, groups, bots, channel stats | QR, phone, bot |
| | Discord | `discord` | servers, channels, DMs, search | QR, token |
| | WhatsApp | `whatsapp` | chats, groups, Cloud API | QR, token |
| | QQ | `qq`, `onebot` | official bots; NapCat / Lagrange through OneBot | token |
| | Feishu / Lark 飞书 | `feishu` | chats, messages, events | app |
| | DingTalk 钉钉 | `dingtalk` | robot messages, events | app, webhook |
| | WeCom 企业微信 | `wecom` | app messages, 客服, AI bot events | app, webhook |
| | Slack | `slack` | channels, threads, search, Socket Mode | token |
| | Mattermost | `mattermost` | teams, channels, threads, search | token |
| | Rocket.Chat | `rocketchat` | rooms, threads, search | token |
| | Matrix | `matrix` | rooms, spaces, public directory | token, password |
| | KOOK | `kook` | servers, channels, DMs | token |
| | VoceChat | `vocechat` | channels, direct chats | password, key |
| | LINE | `line` | push, broadcast, audience insights | token |
| | Satori | `satori` | any Satori server | token |

## Quick start

Build from source (Rust 1.88+, plus `cmake` and `clang` for the TLS library;
`ffmpeg` is optional and used for merging video and audio):

```sh
cargo install --path .
```

Try it without an account:

```sh
social bili search "rust 教程" -n 5
social bili comments '#1'
social x user-posts NASA -n 10
social xhs read https://www.xiaohongshu.com/explore/68cfff45000000001003c253
social reddit sub rust --sort top --time week -n 5
social youtube search "rust tutorial" --filter video -n 5
social yt transcript https://youtu.be/5C_HPTJg5ek
social mp read https://mp.weixin.qq.com/s/nwv1BVYGbpuYJO0bPp-APQ
social tg messages @telegram -n 20
social misskey feed --type global -n 10
```

Log in for everything else:

```sh
social bili login                 # scan the QR code with the Bilibili app
social tg login                   # scan in Telegram → Devices → Link Desktop Device
social twitter login --cookie 'auth_token=...; ct0=...'
social reddit login --browser     # reuse the session of your browser
social slack login --token xoxb-...
social zhihu status
```

A few more:

```sh
social zhihu hot -n 10
social xhs search 咖啡 --sort popular
social x read https://x.com/NASA/status/2104695667180380260 --json
social bili subtitle BV1xx411c7mD
social bili download BV1xx411c7mD --audio-only --split 25
social twitter post "Hello" -i photo.jpg
social reddit comments '#1' --sort top
social reddit post "Hello from the terminal" --title "Hi" --topic r/test
social reddit follow r/rust
social youtube user @Fireship
social yt comments '#1' --sort new --all --replies -f csv > comments.csv
social youtube download https://www.youtube.com/shorts/fwBIZRq-vzY
social tg chats                                   # your dialogs; then:
social tg messages '#3' -n 500 --since 7d -f jsonl > group.jsonl
social discord send '#2' 'Hello' -a chart.png
social feishu listen -f jsonl                     # events, as they arrive
```

Each platform also answers to shorter names (`bilibili`, `x`, `xiaohongshu`,
`zh`, `rd`, `yt` ...). Link the binary as `bili`, `xhs`, `twitter`, `zhihu`,
`reddit` or `youtube` to skip the platform word entirely: `ln -s $(which social) ~/.local/bin/bili`.

## Documentation

Everything is documented inside the binary:

```sh
social guide                 # topics: start, login, refs, output, analysis, download ...
social guide analysis        # creator insights, exports, snapshots
social guide bili            # one platform: commands, references, login, limits
social bili search --help    # a command's options, with examples
```

## Good to know

- `SOCIAL_<PLATFORM>_COOKIE` (for example `SOCIAL_BILI_COOKIE`) supplies a cookie
  header without saving it. `--proxy` or `HTTPS_PROXY` routes requests through a proxy.
- `--interval 5` (or `SOCIAL_INTERVAL=5`) spaces requests at least five seconds
  apart, for large collection jobs.
- Sessions live in `~/.config/social-cli/<platform>/session.json` (readable by
  you only); `social <platform> logout` removes them.
- Bot tokens, API keys and app credentials go in with `login --token`
  (`-` reads them from stdin); `social guide <platform>` says which form a
  platform takes and what it unlocks.
- `login --browser` reads the session from a local browser (Chrome, Edge,
  Firefox, Brave ...). To build without it, use `--no-default-features`.
- Reddit refuses logged-out requests from some networks. Log in, or set
  `REDDIT_CLIENT_ID`, `REDDIT_CLIENT_SECRET`, `REDDIT_USERNAME` and
  `REDDIT_PASSWORD` for a personal "script" app from
  <https://www.reddit.com/prefs/apps> to use Reddit's official API.
- YouTube's creator analytics (`social youtube insights`) come from the
  official YouTube Analytics API: set `YOUTUBE_CLIENT_ID` and
  `YOUTUBE_CLIENT_SECRET` of a "Desktop app" OAuth client, run
  `social youtube oauth` once and set the `YOUTUBE_REFRESH_TOKEN` it prints.
- Keep bulk jobs slow: the platforms watch for automated use and may restrict
  accounts that look like bots. Automating a personal Discord or WhatsApp
  account is against their terms; bots and official APIs are the safe route.

## License

MIT
