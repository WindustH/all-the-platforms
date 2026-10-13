<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/social-cli-logo-dark.svg">
    <img src="docs/assets/social-cli-logo-light.svg" alt="social-cli" width="360">
  </picture>
</p>

<p align="center">
  <strong>一个命令行，玩转 31 个社交、视频和聊天平台。</strong>
</p>

<p align="center">
  <a href="docs/output.md">输出格式</a> ·
  <a href="docs/architecture.md">架构</a> ·
  <a href="README.md">English</a>
</p>

---

`social` 用同一套命令在知乎、小红书、微博、哔哩哔哩、抖音、微信、Telegram、
Discord、YouTube、TikTok、Reddit、Twitter / X 等平台上浏览、搜索、发布、聊天和下载。
在终端里给人看，在脚本和 AI Agent 里则输出稳定的 JSON / YAML。

```console
$ social bili search "rust 教程" -n 3
 #  Content                                                      Author        Likes  Cmts  Views   Time
 1  Rust编程语言入门教程（Rust语言/Rust权威指南配套）【已完结】  软件工艺师    26.1k  2.6k    1.7M  2020-10-21
 2  【Rust腐蚀 新手教程】2026保姆级教程 实时更新！！！           Utaoki丶苦药   1.9k   401  155.3k  2025-01-13
 3  Rust语言 Slint教程，非tauri                                  猩猩程序员        0     0    7.5k

$ social bili read '#1'
$ social tg listen -f jsonl          # 实时输出每一条新的 Telegram 消息
```

## 为什么用 social-cli

- **学一次，到处用。** `search`、`hot`、`feed`、`read`、`comments`、`user`、
  `like`、`comment`、`follow`、`post`、`download` 等命令在每个平台含义相同，
  聊天类平台共用 `chats`、`messages`、`send`、`react` 和 `listen`。
  `social platforms` 列出各平台支持的功能。
- **为 Agent 和脚本设计。** 所有结果都装在同一个信封里，字段和错误码稳定；
  输出被管道接走时自动切换为 YAML。帖子、消息、用户、评论在各平台是同一种结构。
- **直接用于数据分析。** 创作者后台数据（`insights`：趋势、流量来源、观众画像）、
  点赞和转发的人、完整的评论楼中楼、聊天记录导出、`--since` / `--until` 时间过滤，
  以及可直接导入 pandas / DuckDB 的 CSV、JSON Lines 输出。
- **实时事件。** `listen` 实时输出聊天、群组和频道里的新消息，每条一行或一个 JSON 对象。
- **短引用。** 任何列表之后，`#3` 就是「第三条」，不用复制长长的 id 或链接。
- **翻页不丢位置。** `-n 200` 自动翻页；`--cursor` 从上次停下的地方精确继续。
- **登录省事。** 用 App 扫码，沿用浏览器里的登录状态，或填入机器人 Token、API 密钥。
  登录状态只保存在本机。
- **很多功能无需账号。** 大多数平台的公开视频、帖子、评论、用户、频道和文章都能直接看。
- **自带下载。** 原图、合并好音轨的视频、纯音频，或可直接用于语音识别的 WAV 分段。
- **像浏览器一样访问。** 浏览器的网络指纹、各平台的请求签名和克制的请求节奏，
  让请求看起来和普通浏览无异。

## 支持的平台

| | 平台 | 命令 | 主要能力 | 登录方式 |
| --- | --- | --- | --- | --- |
| 社交 | 知乎 | `zhihu` | 回答、文章、想法、提问、创作者数据 | 扫码、浏览器 |
| | 小红书 | `xhs` | 笔记、评论、活动中心、创作者数据 | 扫码、浏览器 |
| | 微博 | `weibo` | 微博、热搜、评论、转发、数据 | 扫码、浏览器 |
| | 百度贴吧 | `tieba` | 吧、帖子、楼层、签到 | 扫码、浏览器 |
| | 微信公众号 | `mp` | 文章、合集、草稿、发表、留言、数据 | 扫码、AppID |
| | Twitter / X | `twitter` | 推文、对话、列表、数据 | 浏览器 |
| | Reddit | `reddit` | 帖子、评论、社区、收件箱 | 浏览器、应用 |
| | Facebook | `facebook` | 个人页、公共主页、小组、主页数据、广告 | 浏览器、Token |
| | Instagram | `instagram` | 帖子、Reels、快拍、私信、数据、广告 | 浏览器、Token |
| | Misskey | `misskey` | 帖子、时间线、表情回应、聊天 | MiAuth、Token |
| 视频 | 哔哩哔哩 | `bili` | 视频、弹幕、字幕、投币、创作者数据 | 扫码、浏览器 |
| | 抖音 | `douyin` | 视频、图文、热榜、无水印下载 | 浏览器 |
| | YouTube | `youtube` | 视频、Shorts、字幕文本、播放列表、数据 | 浏览器 |
| | TikTok | `tiktok` | 视频、音乐、话题、广告 | 扫码、浏览器 |
| 聊天 | 微信（ClawBot） | `wechat` | 接收并回复消息 | 扫码 |
| | Telegram | `telegram` | 对话、频道、群组、机器人、频道数据 | 扫码、手机号、机器人 |
| | Discord | `discord` | 服务器、频道、私信、搜索 | 扫码、Token |
| | WhatsApp | `whatsapp` | 对话、群组、Cloud API | 扫码、Token |
| | QQ | `qq`、`onebot` | 官方机器人；通过 OneBot 连接 NapCat / Lagrange | Token |
| | 飞书 / Lark | `feishu` | 群聊、消息、事件 | 应用 |
| | 钉钉 | `dingtalk` | 机器人消息、事件 | 应用、Webhook |
| | 企业微信 | `wecom` | 应用消息、微信客服、智能机器人事件 | 应用、Webhook |
| | Slack | `slack` | 频道、讨论串、搜索、Socket Mode | Token |
| | Mattermost | `mattermost` | 团队、频道、讨论串、搜索 | Token |
| | Rocket.Chat | `rocketchat` | 房间、讨论串、搜索 | Token |
| | Matrix | `matrix` | 房间、空间、公开目录 | Token、密码 |
| | KOOK | `kook` | 服务器、频道、私信 | Token |
| | VoceChat | `vocechat` | 频道、私聊 | 密码、密钥 |
| | LINE | `line` | 推送、群发、受众数据 | Token |
| | Satori | `satori` | 任意 Satori 服务端 | Token |

## 快速开始

从源码构建（需要 Rust 1.88+，以及构建 TLS 库所需的 `cmake` 和 `clang`；
`ffmpeg` 可选，用于合并视频和音频）：

```sh
cargo install --path .
```

不登录先试试：

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

登录后解锁全部功能：

```sh
social bili login                 # 用 B 站 App 扫码
social tg login                   # 在 Telegram「设备 → 连接桌面设备」中扫码
social twitter login --cookie 'auth_token=...; ct0=...'
social reddit login --browser     # 沿用浏览器里的登录状态
social slack login --token xoxb-...
social zhihu status
```

更多例子：

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
social tg chats                                   # 你的对话列表，然后：
social tg messages '#3' -n 500 --since 7d -f jsonl > group.jsonl
social discord send '#2' 'Hello' -a chart.png
social feishu listen -f jsonl                     # 实时输出事件
```

每个平台还有更短的名字（`bilibili`、`x`、`xiaohongshu`、`zh`、`rd`、`yt` 等）。把程序链接成
`bili`、`xhs`、`twitter`、`zhihu`、`reddit` 或 `youtube`，就能省掉平台名：
`ln -s $(which social) ~/.local/bin/bili`。

## 文档

完整的使用文档内置在程序里：

```sh
social guide                 # 主题列表：start、login、refs、output、analysis、download ...
social guide analysis        # 创作者数据、导出、定时快照
social guide bili            # 单个平台：命令、引用格式、登录、限制
social bili search --help    # 单个命令的选项和示例
```

（内置文档为英文。）

## 须知

- `SOCIAL_<平台>_COOKIE`（例如 `SOCIAL_BILI_COOKIE`）可以临时提供 Cookie 而不保存。
  `--proxy` 或 `HTTPS_PROXY` 让请求走代理。
- `--interval 5`（或 `SOCIAL_INTERVAL=5`）让请求之间至少间隔 5 秒，适合大批量采集。
- 登录状态保存在 `~/.config/social-cli/<平台>/session.json`（仅本人可读），
  `social <平台> logout` 即可删除。
- 机器人 Token、API 密钥和应用凭证通过 `login --token` 填入（`-` 表示从标准输入读取）；
  `social guide <平台>` 说明该平台需要哪种格式、能解锁哪些功能。
- `login --browser` 从本机浏览器（Chrome、Edge、Firefox、Brave 等）读取登录状态。
  不需要这个功能时可用 `--no-default-features` 构建。
- Reddit 会拒绝部分网络的未登录请求。可以先登录，或者在
  <https://www.reddit.com/prefs/apps> 创建一个个人 "script" 应用，并设置
  `REDDIT_CLIENT_ID`、`REDDIT_CLIENT_SECRET`、`REDDIT_USERNAME` 和
  `REDDIT_PASSWORD`，改用 Reddit 官方 API。
- YouTube 的创作者数据（`social youtube insights`）来自官方 YouTube Analytics API：
  设置一个「桌面应用」OAuth 客户端的 `YOUTUBE_CLIENT_ID` 和 `YOUTUBE_CLIENT_SECRET`，
  运行一次 `social youtube oauth`，再设置它输出的 `YOUTUBE_REFRESH_TOKEN`。
- 批量操作请放慢节奏：各平台都会监测自动化行为，可能限制看起来像机器人的账号。
  自动操作个人 Discord 或 WhatsApp 账号违反其服务条款；机器人和官方 API 更安全。

## 许可证

MIT
