An internal app's **robot** (企业内部应用机器人), plus custom group robots
(自定义机器人 webhooks). DingTalk gives robots no chat list and no history:
they send, recall, check who read what they sent, and receive messages live.

## Logging in

1. In the developer console (open-dev.dingtalk.com) create an internal app,
   add the 机器人 ability with **Stream 模式** as the message receiving
   mode, and grant the permissions you need (robot messages, 通讯录 for
   `user`, 场景群 for `group` / `members`). A missing one fails with
   `permission_denied` and a hint to grant it (权限管理).
2. Run:

       atp dingtalk login --token APPKEY:APPSECRET[:ROBOTCODE]

   The robot code is usually the AppKey, which is the default when it is
   left out.

`login` checks the AppKey and AppSecret by fetching an access token.
`whoami` shows the app key and robot code (DingTalk has no "who am I" for
robots).

The access token lasts about two hours. ATP caches it privately, renews it
shortly before it expires and once more when DingTalk calls it invalid. The
AppKey and AppSecret do not expire here; when DingTalk refuses them,
commands fail with `not_authenticated` and the login hint.

Environment variables, which win over the saved login for that run:

- `DINGTALK_APP_KEY` and `DINGTALK_APP_SECRET` (or `DINGTALK_CLIENT_ID`
  and `DINGTALK_CLIENT_SECRET`): the app.
- `DINGTALK_ROBOT_CODE`: the robot code (default: the AppKey).
- `DINGTALK_WEBHOOK_SECRET`: the 加签 secret for a webhook URL given
  without `&secret=`.

### Without logging in

Robot webhooks need no login at all: `send` to a webhook URL or
`webhook:TOKEN[:SECRET]` works on its own. A 加签 robot needs its secret;
without it DingTalk refuses with `permission_denied`.

## References

- Groups: the `openConversationId` (`cid…`). It is in every message
  `listen` prints (`chat`), or comes from a scene group.
- Users: a staff id, `user:ID` (or the bare id).
- Robot webhooks: `https://oapi.dingtalk.com/robot/send?access_token=…`,
  plus `&secret=SEC…` for robots with 加签, or `webhook:TOKEN[:SECRET]`.
  The `session_webhook` of a received message works too.
- Sent messages: `CHAT/processQueryKey`, as `send` prints them.

## Commands

### send

`send CHAT "text" [-a FILE ...]` to a group, a user or a webhook. The text
goes first, then each file as its own message: JPEG, PNG and GIF images as
images, other files as files. Webhooks take text and Markdown only (text
with Markdown marks goes as Markdown), no files. Replies to a specific
message (`--reply-to`) are not possible.

A received message's `session_webhook` replies to its conversation for
about 90 minutes; `extra.reply_to_chat` names the chat to send a reply to
(the group, or `user:STAFFID` for a direct message).

### read

`read CHAT/processQueryKey`: a sent message's delivery status
(`extra.send_status`) and who read it (`extra.read_by` in groups,
`extra.readers` for users). Webhook messages cannot be read back.

### delete

Recalls a message the robot sent, by the id `send` printed. Webhook
messages cannot be recalled.

### user

`user STAFFID`: a staff member's profile (needs the 通讯录 permission).

### members

`members cid…`: scene groups (场景群) only; ordinary groups have no member
API for robots.

### listen

`listen [cid… ...]` connects in Stream mode (no public URL needed) and
prints messages sent to the robot (group mentions and direct messages) and
every other event the app subscribed to, with the event type as `kind`.
After five failed connections in a row it gives up; check that the robot
uses Stream mode.

### group

`group cid…`: details of a scene group (场景群).

## Limits

- No chat history, no chat list, no reactions or edits: DingTalk's robot
  APIs do not have them.
- ATP keeps at least 250 ms between requests; DingTalk's throttling comes
  back as `rate_limited`.
