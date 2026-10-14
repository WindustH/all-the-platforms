A Feishu (飞书) or Lark app's **bot**: the chats it was added to, their
messages, sending, and live events. It is not a personal account: the
Open Platform gives apps a tenant token, and user-level features (your own
private chats, message search) are out of its reach.

## Logging in

1. Create an internal app (企业自建应用) in the developer console
   (open.feishu.cn/app or open.larksuite.com/app) and enable 机器人 (Bot)
   under 应用能力. Without it commands fail with "the app has no bot
   ability".
2. Grant the scopes you need (权限管理) and publish a version:
   - `im:message` / `im:message:send_as_bot` to send;
   - `im:message.group_msg` (获取群组中所有消息) and `im:message.p2p_msg`
     to read history;
   - `im:chat` / `im:chat:readonly` for chats and members;
   - `im:resource` for files; `contact:user.base:readonly` for names.

   A missing scope fails with `permission_denied` naming it; add it,
   publish a new version and retry.
3. Take the app's App ID (it starts with `cli_`) and App Secret and run:

       atp feishu login --token APP_ID:APP_SECRET

   Prefix `lark:` (`lark:APP_ID:APP_SECRET`) for an app on larksuite.com.

`login` checks the credentials by asking for the bot's info; `whoami` then
shows the app id, the host and whether the bot is activated.

The tenant access token lasts about two hours. ATP caches it privately,
renews it shortly before it expires and once more when Feishu calls it
invalid. The App ID and App Secret do not expire here; when Feishu refuses
them, commands fail with `not_authenticated` and the login hint.

Environment variables, which win over the saved login for that run:

- `FEISHU_APP_ID` and `FEISHU_APP_SECRET`: a Feishu app;
  `FEISHU_BASE_URL` points them at another host (default
  `https://open.feishu.cn`).
- `LARK_APP_ID` and `LARK_APP_SECRET`: a Lark app (used when the Feishu
  pair is not set).

Nothing works without logging in.

## References

- Chats `oc_…` (from `chats`, `search-chats` or `#N`), threads `omt_…`
  (`messages omt_…`).
- Messages `om_…`: ids are global, so `read`, `edit`, `delete`, `react`,
  `download`, `pin` and `forward` need nothing else (`CHAT/om_…` works
  too).
- Recipients of `send` and `forward`: a chat (`oc_…`) or a user (`ou_…`
  open id, `on_…` union id, an email, or a user id).
- Users for `user`: `ou_…`, `on_…` or a user id.

## Commands

### chats

The chats the bot is in, most recently active first. Chats have no
channels inside them; a topic group's thread is read with `messages omt_…`.

### members

Members of a chat, by open id (`ou_…`).

### messages

`messages CHAT -n 200 --since 7d` reads history, newest first (needs the
history scopes above). Authors get names when the contact scope is there;
without it they keep their open ids. `messages omt_…` reads a thread.

### read

One message by its `om_…` id, with the chat's name.

### send

`send CHAT "text" [-a FILE ...] [--reply-to MSG]`: the text first, then one
message per file. Images (jpg, png, gif, webp, bmp, ico, tiff, heic) go as
images, `.opus` as voice, everything else as files. With `--reply-to` the
first message answers that message in its chat.

### edit

Replaces the text of a text message the bot sent.

### delete

Recalls a message: the bot's own, or any in a group it manages, following
Feishu's rules (time limits, group admin rights).

### react

`react MSG 👍 [--undo]`: 👍 👌 😄 or a Feishu reaction name such as
`THUMBSUP`. `--undo` removes the bot's own reaction of that kind.

### download

Saves a message's image or file.

### listen

`listen [CHAT ...]` uses the long connection (长连接): no public URL is
needed. In the developer console choose 使用长连接接收事件 under 事件与回调
and add the events: `im.message.receive_v1` (messages),
`im.message.recalled_v1`, `im.message.reaction.created_v1` /
`deleted_v1`, `im.message.message_read_v1`. Other events arrive with
their type as `kind`.

Feishu limits how many long connections an app may hold; too many fail
with `rate_limited` (close other `listen` runs or SDK clients of the same
app). After five failed connections in a row `listen` gives up; check
that the app receives events through the long connection.

### chat

`chat CHAT`: one chat's details.

### search-chats

`search-chats QUERY`: chats visible to the bot whose name matches.

### pin, pins

`pin MSG [--undo]` pins a message in its chat; `pins CHAT` lists the pinned
messages with who pinned them (`extra.pinned_by`).

### forward

`forward MSG TO`: to a chat or a user, in the forms `send` takes; prints
the new message's id.

### read-by

`read-by MSG`: who has read a message the bot sent, with `extra.read_at`.

## Limits

- Bots see only chats they are in; they cannot search messages (a user
  token is needed for that) and cannot list users' private chats.
- ATP keeps at least 250 ms between requests; Feishu's frequency limits
  come back as `rate_limited`.
- A bot outside the chat gets `permission_denied` ("add the bot to the
  chat first").
