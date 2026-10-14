Bangumi (番组计划, bgm.tv) through its official API (`api.bgm.tv`):
subjects of every type (anime, books, music, games and real-life shows),
their characters, staff, related subjects and episodes, the airing
calendar, users and their collections. A personal access token adds your
own collection: states, ratings, comments, tags and progress.

## Logging in

### Personal access token

1. Sign in on https://bgm.tv and open https://next.bgm.tv/demo/access-token.
2. Create a token there.
3. `atp bangumi login --token TOKEN` (`-` reads it from stdin). The token
   is checked against your account at once (`/v0/me`) and kept in ATP's
   session file (readable by you only). `BANGUMI_TOKEN` supplies one for a
   single run instead and is never saved.

The token unlocks `whoami`, `library` without a user name (your own
collection, private entries included), `mark` and `progress`. It also
lets search show NSFW subjects when your account may see them.

### Without an account

`search`, `hot`, `read`, `user`, `characters`, `persons`, `relations`,
`episodes`, `browse` and `library USER` work without a token. Private
collection entries and NSFW subjects stay hidden.

### How long it lasts

How long a token stays valid is set when it is created on the token
page; ATP cannot tell. A token Bangumi no longer accepts fails with
`not_authenticated`; create a new one and log in again.

### Limits and pacing

Bangumi publishes no rate limit. ATP spaces requests at least half a
second apart and waits 0.3 to 0.9 seconds before each change to your
collection. Requests name ATP as their client in the `User-Agent`
(`WindustH/atp (https://github.com/WindustH/atp)`), as Bangumi's API rules
ask.

### Environment

`BANGUMI_TOKEN`: a token for one run.

## References

- Subjects: a link (`https://bgm.tv/subject/400602`; bangumi.tv, chii.in
  and next.bgm.tv too), `subject/<id>`, or the id.
- Characters, persons and episodes: their links or paths
  (`https://bgm.tv/character/<id>`, `person/<id>`, `ep/<id>`).
- Users: a profile link (`https://bgm.tv/user/sai`), `user/<name>`,
  `@name` or the name; users without a name go by their number.
- `#N` works after any list.

## Commands

### search

Subjects only. `--filter anime|book|music|game|real` keeps one type;
`--sort match|heat|rank|score` (`match` by default; `heat`: most
collected).

### hot

The airing calendar (每日放送): `--category today` (the default, by your
computer's clock), `mon` … `sun`, or `week` (Monday first). Each subject
has its weekday in `extra.weekday`.

### read

A subject, character, person or episode (by link). Subjects show their
Chinese title (the original in `extra.name`), summary, type, release date,
episode or volume count, score, rank and votes (`extra.score`,
`extra.rank`, `extra.votes`), how many people collected them by state
(`metrics`: `wish`, `collect`, `doing`, `on_hold`, `dropped`), tags and
the infobox (`extra.infobox`).

### user

A profile: nickname, user name, signature, avatar and user number.

### characters, persons, relations

`characters SUBJECT` lists the cast with their voice actors
(`extra.actors`) and role (`extra.relation`); `persons SUBJECT` the staff
with their job; `relations SUBJECT` sequels, adaptations, soundtracks and
other related subjects. Each is a link `read` takes.

### episodes

`episodes SUBJECT [--type main|sp|op|ed|pv|mad|other]`: number, title,
air date, length and comment count of each episode.

### browse

`browse TYPE [--sort rank|date] [--year Y] [--month M]`: subjects of one
type, best-ranked or newest first.

### library

`library [USER] [--type TYPE] [--status wish|done|doing|on-hold|dropped]`:
a collection, the most recently changed entry first. Each subject carries
the entry in `extra.collection` (state, rating, episodes and volumes done,
comment, tags); its time is when the entry last changed, so `--since 30d`
shows what was marked lately.

### mark

`mark SUBJECT STATE [--rating N] [--comment TEXT] [--tag T …] [--private]`
adds a subject to your collection or changes its entry (needs a token).
`--rating 0` removes the rating; tags cannot contain spaces.

### progress

`progress SUBJECT N` marks main episodes 1 to N watched (anime and
real-life shows). For a book, `progress BOOK CHAPTERS [--vol VOLUMES]`
sets the chapters and volumes read; the book has to be in your collection
first (`mark`).

## Limits

No comments, reviews, discussion boards, groups or blogs (the API has
none), no character or person search, no removing a subject from your
collection (Bangumi's API cannot), and no marking single episodes other
than "watched up to N".
