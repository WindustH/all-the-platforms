## What this is

A WeCom (企业微信) **corp app** (自建应用) and, optionally, an **AI bot**
(智能机器人) in long-connection mode; group robot webhooks work without
either. WeCom gives apps no access to ordinary chat history (that is 会话存档,
a paid add-on with its own SDK), so reading is limited to 微信客服
conversations.

## Logging in

- App: in the admin console (work.weixin.qq.com → 应用管理) take the corp
  id (我的企业), the app's Secret and AgentId, add this machine's IP to the
  app's 企业可信IP, then
  `atp wecom login --token CORPID:SECRET:AGENTID`.
- AI bot: create one with 长连接 (API mode), then
  `atp wecom login --token bot:BOTID:SECRET`. Both logins can be kept.
- WECOM_CORP_ID / WECOM_SECRET / WECOM_AGENT_ID and WECOM_BOT_ID /
  WECOM_BOT_SECRET work for one run and win over the saved ones.

## Sending

`send CHAT "text" [-a FILE ...]`, where CHAT is:

- users `user:ID` (or a bare id; `user:A|B` for several), a department
  `party:ID`, a tag `tag:ID`, or `@all` — as the app (needs the agent id);
- an app group chat `chat:CHATID` (see `create-chat`);
- a 微信客服 customer `kf:OPEN_KFID/EXTERNAL_USERID`;
- a group robot webhook `https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=…`
  or `webhook:KEY`.

Text with Markdown marks goes as Markdown. Images go as images, `.amr` as
voice, `.mp4` as video, other files as files. App and 客服 messages print
their id `TARGET/MSGID`; `delete` recalls them (app messages within 24 h).

## Reading

- `chats`: the 微信客服 accounts (`kf:OPEN_KFID`).
- `messages kf:OPEN_KFID`: their conversations, oldest first (WeCom limits
  how often this may run without a callback token).
- `members chat:CHATID` / `members party:ID`, `departments [PARENT]`,
  `chat chat:CHATID`, `user ID`, `whoami` (the app).

## Live messages

`listen` needs the AI bot: messages to it (direct and group mentions) and
its events arrive over the long connection. Images in them are encrypted
with the bot's key; their URLs are in `extra.image_url`.

## Limits

No ordinary chat history, no reactions, no edits, no replies to a message;
robot webhook and app group chat messages cannot be recalled.
