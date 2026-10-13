## What this is

A Feishu (飞书) or Lark app's **bot**: the chats it was added to, their
messages, sending, and live events. It is not a personal account: the
Open Platform gives apps a tenant token, and user-level features (your own
private chats, message search) are out of its reach.

## Logging in

1. Create an internal app (企业自建应用) in the developer console
   (open.feishu.cn/app or open.larksuite.com/app) and enable 机器人 (Bot).
2. Grant the scopes you need (权限管理) and publish a version:
   - `im:message` / `im:message:send_as_bot` to send;
   - `im:message.group_msg` (获取群组中所有消息) and `im:message.p2p_msg`
     to read history;
   - `im:chat` / `im:chat:readonly` for chats and members;
   - `im:resource` for files; `contact:user.base:readonly` for names.
3. `atp feishu login --token APP_ID:APP_SECRET` (prefix `lark:` for
   larksuite.com). FEISHU_APP_ID / FEISHU_APP_SECRET (or LARK_APP_ID /
   LARK_APP_SECRET) work for one run and win over the saved ones.

## References

- Chats `oc_…` (from `chats` or `#N`), threads `omt_…` (`messages omt_…`).
- Messages `om_…`: ids are global, so `read`, `edit`, `delete`, `react`
  and `download` need nothing else.
- `send` takes a chat (`oc_…`), a user (`ou_…` open id, `on_…` union id,
  an email, or a user id).

## Chats and messages

- `chats` lists the chats the bot is in; `chat CHAT` shows one;
  `search-chats QUERY` finds chats by name; `members CHAT`.
- `messages CHAT -n 200 --since 7d` reads history, newest first (needs the
  history scopes above). Authors get names when the contact scope is there.
- `send CHAT "text" [-a FILE ...] [--reply-to MSG]`: images go as images,
  `.opus` as voice, everything else as files; one message per file.
- `edit MSG "text"` (the bot's text messages), `delete MSG` (recall),
  `react MSG 👍 [--undo]` (👍 👌 😄 or a Feishu name such as `THUMBSUP`),
  `pin MSG [--undo]`, `pins CHAT`, `forward MSG TO`, `read-by MSG`.
- `download MSG` saves a message's image or file.

## Live events

`listen [CHAT ...]` uses the long connection (长连接): no public URL is
needed. In the developer console choose 使用长连接接收事件 under 事件与回调
and add the events: `im.message.receive_v1` (messages),
`im.message.recalled_v1`, `im.message.reaction.created_v1` /
`deleted_v1`, `im.message.message_read_v1`. Other events arrive with
their type as `kind`.

## Limits

- Bots see only chats they are in; they cannot search messages (a user
  token is needed for that) and cannot list users' private chats.
- Edits apply to text messages the bot sent; recalls follow Feishu's
  rules (time limits, group admin rights).
