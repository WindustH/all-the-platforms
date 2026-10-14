A WeCom (企业微信) **corp app** (自建应用) and, optionally, an **AI bot**
(智能机器人) in long-connection mode; group robot webhooks work without
either. WeCom gives apps no access to ordinary chat history (that is 会话存档,
a paid add-on with its own SDK), so reading is limited to 微信客服
conversations.

## Logging in

There are two logins, and both can be kept at once.

### Corp app

1. In the admin console (work.weixin.qq.com) take the corp id (我的企业),
   and the app's Secret and AgentId (应用管理 → the app).
2. Add this machine's public IP to the app's 企业可信IP. Calls from other
   addresses fail with `ip_blocked` ("this IP is not trusted by the app").
3. Run:

       atp wecom login --token CORPID:SECRET:AGENTID

The agent id may be left out (`CORPID:SECRET`); sending as the app then
fails until it is given. `login` checks the app by fetching an access token
and its details.

The app sees and reaches only what its 可见范围 (visible range) and API
permissions allow; other calls fail with `permission_denied` and a hint to
check them in the admin console.

### AI bot

Create an AI bot (智能机器人) with 长连接 (API mode), then:

    atp wecom login --token bot:BOTID:SECRET

The bot is only for `listen`. Its credentials are checked when `listen`
connects ("WeCom refused the AI bot" otherwise).

### Without logging in

`send` to a group robot webhook (`webhook:KEY` or its URL) needs no login.

### Sessions and environment

The app's access token lasts about two hours; ATP caches it privately,
renews it before it expires and once more when WeCom calls it invalid. The
corp id and secret do not expire here; when WeCom refuses them, commands
fail with `not_authenticated` and the login hint.

Environment variables, which win over the saved logins for that run:

- `WECOM_CORP_ID`, `WECOM_SECRET` (both needed) and `WECOM_AGENT_ID`: the app.
- `WECOM_BOT_ID` and `WECOM_BOT_SECRET`: the AI bot.

## References

- Users: `user:ID` (or a bare id; `user:A|B` names several), a department
  `party:ID`, a tag `tag:ID`, or `@all`.
- App group chats: `chat:CHATID` (see `create-chat`).
- 微信客服 accounts: `kf:OPEN_KFID`, from `chats`; one customer's
  conversation: `kf:OPEN_KFID/EXTERNAL_USERID`.
- Group robot webhooks: `https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=…`
  or `webhook:KEY`.
- Sent messages: `TARGET/MSGID`, as `send` prints them.

## Commands

### chats

The 微信客服 accounts (`kf:OPEN_KFID`). Chats have nothing inside them.

### messages

`messages kf:OPEN_KFID`: what customers and servicers of that 客服 account
exchanged, oldest first. WeCom limits how often this may run without a
callback token. Other chats have no readable history.

### members

`members chat:CHATID` (an app group chat) or `members party:ID` (the users
of a department).

### user, whoami

`user ID`: a corp user. `whoami` shows the app (or the corp when no agent
id is known, or the AI bot when only it is logged in).

### send

`send CHAT "text" [-a FILE ...]`, where CHAT is users, a department, a tag
or `@all` (as the app, needs the agent id), an app group chat `chat:…`, a
微信客服 customer `kf:OPEN_KFID/EXTERNAL_USERID`, or a robot webhook.

Text with Markdown marks goes as Markdown. JPEG and PNG images go as
images, `.amr` as voice, `.mp4` as video, other files as files, each as its
own message. App and 客服 messages print their id `TARGET/MSGID`. There is
no `--reply-to`: apps and robots cannot quote a message. Recipients WeCom
could not reach are logged as a warning.

### delete

Recalls app messages (within 24 h) and 微信客服 messages, by the
`TARGET/MSGID` `send` printed. Robot webhook and app group chat messages
cannot be recalled.

### listen

Needs the AI bot: messages to it (direct and group mentions) and its events
arrive over the long connection (`wss://openws.work.weixin.qq.com`), so no
public callback URL is needed. Ordinary app callbacks are not supported.
Images in bot messages are encrypted with the bot's key; their URLs are in
`extra.image_url`. Direct messages carry the chat `user:ID`, group ones the
group's chat id.

### departments

`departments [PARENT]`: all departments, or those under one (`party:ID` or
its id).

### chat

`chat chat:CHATID`: details of an app group chat.

### create-chat

`create-chat NAME USER USER… [--owner USER]` creates an app group chat
(at least two users) and prints its `chat:CHATID`.

## Limits

- No ordinary chat history, no reactions, no edits, no replies to a message.
- ATP keeps at least 250 ms between requests; WeCom's frequency limits
  (codes 45009, 45033, 45047) come back as `rate_limited`.
- A wrong or removed robot webhook key comes back as `not_found`.
