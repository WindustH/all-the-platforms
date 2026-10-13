## Logging in

`social line login --token CHANNEL_ACCESS_TOKEN` with a long-lived channel
access token (LINE Developers console → your channel → Messaging API), or
`LINE_CHANNEL_ACCESS_TOKEN` for one run. The token is checked with
`whoami` (the bot's name, basic id and friend count).

## What a bot can do

LINE delivers what users send to a bot only by webhook, so there is no
history, no chat list and no `listen` here; message ids, user / group / room
ids and quote tokens come from those webhook events.

- `send CHAT TEXT [--reply-to QUOTE_TOKEN]`: CHAT is a user (`U…`), group
  (`C…`) or room (`R…`) id (push), several users `U…,U…` (multicast, up to
  500), or `all` (broadcast to every friend). `image CHAT URL [--preview
  URL]` sends a picture by public HTTPS URL (LINE does not take uploads);
  `sticker CHAT PACKAGE STICKER`.
- `user U…` (profile), `group C…` (name, picture, members), `members C…|R…`
  and `followers` (verified or premium accounts only), `leave C…|R…`.
- `download MESSAGE_ID` saves what a user sent (image, video, audio, file).
- `insights [--days N]`: friends, targeted reach and blocks per day (data
  is ready the day after), yesterday's deliveries by kind, the audience
  (from 20 friends) and the monthly quota. `insights REQUEST_ID` for one
  broadcast: impressions, clicks, media plays (the request id is printed
  when you broadcast).
- `quota` (limit, usage, webhook URL), `richmenus`.

Pushes count against the monthly message quota of the account's plan.
