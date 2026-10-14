Web feeds read directly, with no service in between: RSS (0.9x, 1.0,
2.0), Atom and JSON Feed, podcast and Media RSS extensions included. ATP
keeps a small subscription list of its own and merges it into one
timeline; RSSHub routes turn sites without feeds into feeds on the RSSHub
instance you choose; a Miniflux account of yours can stand in for the
local list.

## Logging in

### Without an account

Reading needs no account and no service: `read`, `user`, `user-posts`
(alias `posts`), `hub`, `download`, and the local subscription list
(`add`, `remove`, `list`, `feed`, `search`). The list lives in
`feeds.json` beside ATP's session file for `rss` (readable by you only);
`logout` leaves it alone.

### Miniflux

Miniflux is a feed reader you host (or rent). Logging in makes your
account the timeline:

1. In Miniflux, open Settings → API Keys and create a key.
2. `atp rss login --token 'URL|API_KEY'`, `URL` being the address of
   your Miniflux (`https://reader.example.com`; `-` reads the value from
   stdin). ATP checks the key (`/v1/me`) and keeps both in its session
   file (readable by you only).

The account adds `feed --type unread|all|starred` (`unread` becomes the
default), `search` across the account, `favorites` and `favorite` (stars),
`mark`, and `add` / `remove` / `list` with `--remote`. A key works until
you delete it in Miniflux; a refused key fails with `not_authenticated`.
`MINIFLUX_URL` and `MINIFLUX_API_KEY` supply an account for one run
instead and are never saved.

FreshRSS (its Google Reader API) is not supported yet.

### RSSHub

RSSHub routes (`rsshub:/github/trending/daily`, `hub ROUTE`) go to
`RSSHUB_INSTANCE` when set, else https://rsshub.app. The public
rsshub.app can answer with a Cloudflare browser check, which no feed
reader passes (the command then fails with `permission_denied`); public
instances also limit requests and leave some routes out. Your own
instance has none of that. `hub --instance URL` picks one for a single
command.

### Fetching and pacing

Feeds are fetched as feed readers fetch them: requests name ATP as their
client (`User-Agent`) and carry the `ETag` / `Last-Modified` of the last
copy, which ATP keeps in its private cache for 30 days, so an unchanged
feed answers `304 Not Modified` and costs its server almost nothing. The
merged timeline fetches four feeds at a time; a feed that fails is
reported and skipped. Requests are spaced at least 0.2 seconds apart.

### Environment

- `MINIFLUX_URL`, `MINIFLUX_API_KEY`: a Miniflux account for one run.
- `RSSHUB_INSTANCE`: the RSSHub instance (default https://rsshub.app).

## References

- Feeds: a URL; a web page that announces its feed
  (`<link rel="alternate" type="application/rss+xml">`), which is
  followed to the feed; a host and path without `https://`; or an RSSHub
  route `rsshub:/…`. `#N` after `list` refers to a subscription.
- Entries: the link an entry list shows (ATP remembers which entry each
  listed link was), `#N` after any entry list, `FEED_URL#ENTRY_ID` (the
  `ref` in `extra`), or `miniflux:<id>`.

## Commands

### read

An entry: title, author, time, text (HTML rendered as text), its
attachments in `media`, its feed in `extra.feed`. A feed: its title,
description, site and its 20 newest entries, numbered, in the text and
in `extra.entries`.

### user, user-posts

`user FEED` shows a feed as a source (title, site, description, entry
count, format); `user-posts FEED` (alias `posts`) lists its entries, the
newest first. `--since` / `--until` work on publication times.

### feed

`--type local|unread|all|starred`: the entries of all local
subscriptions merged, the newest first (`local`, the default without a
Miniflux account), or the Miniflux account's unread (its default), all
or starred entries.

### search

Words in the titles and text of the local subscriptions' entries
(ignoring case), or, with a Miniflux account, Miniflux's search;
`--filter local` searches the local subscriptions even then.

### add, remove, list

`add FEED [--title NAME]` subscribes after fetching the feed once to
check it; routes stay routes, so they follow `RSSHUB_INSTANCE`.
`remove FEED` and `list` act on the local list. With `--remote` they act
on the Miniflux account instead: `add --remote [--category ID]` (the
first category by default), `remove --remote FEED_ID_OR_URL` (asks
first; `-y` in scripts), `list --remote` with unread counts.

### hub

`hub ROUTE [--instance URL]` lists the entries of an RSSHub route, as
`posts rsshub:/ROUTE` does.

### favorite, favorites

Star (`favorite ENTRY`) or unstar (`--undo`) an entry of the Miniflux
account; `favorites` lists the starred ones. Needs the account.

### mark

`mark ENTRY… [--unread]` marks Miniflux entries read (or unread).

### download

The attachments of an entry (podcast episodes, images), as the shared
`download` saves them.

## Limits

No FreshRSS yet, no OPML import or export, no feed folders in the local
list, and no Miniflux categories, feed refreshes or settings beyond
`add --category`. Entries that a feed has dropped can no longer be read
by their link.
