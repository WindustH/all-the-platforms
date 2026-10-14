LINE works through the Messaging API of a LINE Official Account, as its
bot. LINE delivers what users send to a bot only by webhook, so there is no
history, no chat list and no `listen` here.

## Logging in

### Channel access token

    atp line login --token CHANNEL_ACCESS_TOKEN

takes a long-lived channel access token (LINE Developers console → your
channel → Messaging API). It is kept as given and checked with `whoami`
(the bot's name, basic id and friend count). Every command needs it;
nothing works without it. `LINE_CHANNEL_ACCESS_TOKEN` supplies a token for
one run; it wins over the saved one and is never written.

How long a token lasts is not known to ATP. When LINE refuses it, commands fail with `not_authenticated` ("LINE refused
the channel access token"): issue a new channel access token and log in
again (or check `LINE_CHANNEL_ACCESS_TOKEN`).

Member and follower lists need a verified or premium account; without one,
and for calls the channel's settings don't allow, LINE answers with
`permission_denied`.

## References

Ids come from webhook events: message ids, user / group / room ids and
quote tokens.

- Chats: a user (`U…`), group (`C…`) or room (`R…`) id (the letter and 32
  hex digits), several users `U…,U…` (multicast, up to 500), or `all`
  (broadcast to every friend).
- Messages: the id (digits) of a message a user sent, for `download`.
- `--reply-to`: a quote token (from webhook events and sent messages).
- Broadcasts: the request id `send` prints, for `insights`.

## Commands

### send

`send CHAT TEXT [--reply-to QUOTE_TOKEN]` pushes to a user, group or room,
multicasts to `U…,U…`, or broadcasts to `all`. LINE does not take uploads,
so `--attach` fails: send pictures with `image`. A broadcast prints its
request id. A retried send is not delivered twice.

### image

Sends a picture by public HTTPS URL; `--preview URL` gives a smaller
preview image (default: the picture itself).

### sticker

`sticker CHAT PACKAGE STICKER`, with ids from LINE's sticker list.

### user

`user U…` shows a profile; a group id (`C…`) shows the group like `group`.

### group

A group's name, picture and member count.

### members

`members C…|R…` lists a group's or room's members (verified or premium
accounts only). A user id gives the bot and that user.

### followers

The friends of your own bot only (verified or premium accounts):
`followers` or `followers me`.

### leave

Leaves a group (`C…`) or room (`R…`).

### download

`download MESSAGE_ID` saves what a user sent (image, video, audio, file).

### insights

`insights [--days N]` (1 to 30 days): friends, targeted reach and blocks
per day (data is ready the day after, so the days end yesterday, Japan
time), yesterday's deliveries by kind, the audience (from 20 friends) and
the monthly quota. `insights REQUEST_ID` for one broadcast: impressions,
clicks, media plays.

### quota

The monthly message quota, its usage, and the webhook URL (and whether it
is active).

### richmenus

The rich menus and the default one.

## Limits

Pushes count against the monthly message quota of the account's plan.
Requests are spaced at least 100 ms apart, and sends wait a short random
pause (0.2 to 0.6 seconds) first.
