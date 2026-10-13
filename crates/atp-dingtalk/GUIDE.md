## What this is

An internal app's **robot** (企业内部应用机器人), plus custom group robots
(自定义机器人 webhooks). DingTalk gives robots no chat list and no history:
they send, recall, check who read what they sent, and receive messages live.

## Logging in

1. In the developer console (open-dev.dingtalk.com) create an internal app,
   add the 机器人 ability with **Stream 模式** as the message receiving
   mode, and grant the permissions you need (robot messages, 通讯录 for
   `user`, 场景群 for `group` / `members`).
2. `atp dingtalk login --token APPKEY:APPSECRET[:ROBOTCODE]` (the robot
   code is usually the AppKey). DINGTALK_APP_KEY / DINGTALK_APP_SECRET /
   DINGTALK_ROBOT_CODE work for one run and win over the saved ones.

Robot webhooks need no login at all.

## Sending

`send CHAT "text" [-a FILE ...]`, where CHAT is:

- a group's `openConversationId` (`cid…`): it is in every message `listen`
  prints (`chat`), or from a scene group;
- a user's staff id: `user:ID` (or the bare id);
- a robot webhook: `https://oapi.dingtalk.com/robot/send?access_token=…`,
  plus `&secret=SEC…` for robots with 加签 (or DINGTALK_WEBHOOK_SECRET),
  or `webhook:TOKEN[:SECRET]`; the `session_webhook` of a received
  message replies to its conversation for about 90 minutes.

Images go as images, other files as files (not through webhooks, which
take text and Markdown only). Each sent message prints its id
`CHAT/processQueryKey`: `read` shows its delivery status and who read it,
`delete` recalls it.

## Live messages

`listen [cid… ...]` connects in Stream mode (no public URL needed) and
prints messages sent to the robot (group mentions and direct messages) and
every other event the app subscribed to, with the event type as `kind`.

## Other commands

- `user STAFFID`: a staff member's profile.
- `group cid…` and `members cid…`: scene groups (场景群) only.

## Limits

No chat history, no chat list, no reactions or edits: DingTalk's robot APIs
do not have them. Replies to a specific message are not possible either.
