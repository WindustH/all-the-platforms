Xiaoyuzhou (小宇宙), read-only, through the public pages of
www.xiaoyuzhoufm.com: shows with their newest episodes, episodes with
their show notes, play and comment counts, the hot comments an episode
page shows, users with the shows they host, and the episode's audio file,
which `download` saves. The pages carry this data for any visitor; ATP
reads it from them as a browser opening the page does.

## Logging in

### There is no login

ATP does not log in to Xiaoyuzhou, and `atp xiaoyuzhou login` offers no
way in. Everything ATP does works without an account: `read`,
`comments`, `user`, `user-posts` and `download`.

Account features (subscriptions, play history, the editors' picks feed,
liking or writing comments) exist only in Xiaoyuzhou's app, through the
app's logged-in API. ATP stays away from it on purpose: the most used
open-source tool for that API warns in its README that logging in through
it has got accounts flagged or banned. Nothing here touches an account,
so nothing here can get one banned.

### Sessions, tokens, environment

There is no session, no token and no cookie to keep, and no environment
variable. `atp xiaoyuzhou status` always says not logged in.

### Risk control

- The pages are public. ATP keeps at least 1.5 s between requests.
- If Xiaoyuzhou's firewall answers with a check page instead of the page
  (too many requests from one address), commands fail with
  `verification_required` and the page's address; open it in a browser,
  pass the check, and retry later or with a larger `--interval`. How
  often that happens is not known.

## References

- Shows: `https://www.xiaoyuzhoufm.com/podcast/<id>` or `podcast:<id>`.
- Episodes: `https://www.xiaoyuzhoufm.com/episode/<id>` (query
  parameters a shared link carries are ignored) or `episode:<id>`.
- Users (hosts and commenters): `https://www.xiaoyuzhoufm.com/user/<id>`
  or `user:<id>`.
- A bare id (24 hex digits): `read` tries it as a show, then as an
  episode; `user-posts` as a show, then as a user; `comments` and
  `download` take it as an episode.
- Links on other hosts (link shorteners) are followed to the page they
  lead to.
- `#N` works after any list.

## Commands

### read

A show: title, description, author, subscribers
(`metrics.subscribers`), episode count, image, latest release and its
hosts in `extra.podcasters`. An episode: title, the whole show notes as
text (timestamps included), release time, plays (`views`), claps
(`likes`), comments and favorites, the show as `author`, and the audio
file in `media` with its duration and size.

### user-posts

`user-posts SHOW` lists the episodes the show's page lists, the newest
first: the 15 most recent (the website lists no older episodes).
`user-posts USER` lists the shows the user hosts.

### comments

`comments EPISODE`: the hot comments the episode page shows (about 20),
each with the replies the page shows under it (`replies`) and the
commenter's IP region (`location`). There is no paging beyond them and no
newest-first order.

### user

A user's page: name, avatar, bio, IP region, followers, following,
subscriptions (`stats.subscriptions`), listening time
(`extra.played_seconds`) and the shows they host (`extra.shows`).

### download

`download EPISODE [-o DIR]` saves the episode's audio file, the file the
publisher uploaded or Xiaoyuzhou took from the show's feed, as
`<title> [<id>].<ext>`. Paid episodes fail with `permission_denied`:
their audio plays only in the app.

## Limits

- No `search`: the website has no public search, and the app's search
  needs its logged-in API.
- No charts, discovery or editors' picks feed (app only).
- `user-posts` sees only the 15 newest episodes a show's page lists;
  older episodes are reachable by their link only. Shows that also publish an
  RSS feed can be read whole with `atp podcast episodes FEED`.
- `comments` sees only the hot comments of the page, not every comment.
- Paid episodes and paid shows: their listing reads, their audio does
  not download.
- Nothing is written: no subscribing, clapping, favoriting or commenting.
