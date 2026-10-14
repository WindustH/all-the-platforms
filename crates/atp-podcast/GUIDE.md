Podcasts through open sources: Apple's public podcast directory (the
iTunes Search and Lookup APIs and Apple's charts) to find shows and
episodes, each show's own RSS feed for its episodes and show notes, and
optionally Podcast Index with your own API key. `download` saves the
episode file the publisher put in the feed, as any podcast app does.

## Logging in

### Without an account

Everything except Podcast Index works without logging in: `search`
(Apple), `hot` (Apple's charts), `read`, `episodes` and `download`.

### Podcast Index key

1. Sign up at https://api.podcastindex.org (free); the confirmation
   email holds an API key and an API secret.
2. `atp podcast login --token 'KEY:SECRET'` (`-` reads it from stdin).
   ATP signs one request with them (`stats/current`) to check them, then
   keeps them in its session file (readable by you only).

The key adds `search --filter index` and `hot --category trending`. Each
request is signed with SHA-1 of the key, the secret and the current time,
as Podcast Index documents. The key works until you delete it in your
Podcast Index account; ATP cannot tell how long that is. A refused key
fails with `not_authenticated`.

### Limits and pacing

Apple documents about 20 search requests a minute per client; how it
answers beyond that is not known to ATP. Podcast Index publishes no limit
for the free tier. ATP spaces all requests at least a second apart. Feeds
are fetched as a feed reader does: requests name ATP as the client and
carry the validators of the last copy, so an unchanged feed answers
`304 Not Modified` and the copy kept in ATP's private cache is used.

### Environment

- `PODCASTINDEX_KEY`, `PODCASTINDEX_SECRET`: a key and secret for one
  run, never saved.
- `PODCAST_COUNTRY`: the Apple storefront for search and charts, as a
  two-letter code (`us` by default, `cn`, `jp`, `gb` …).

## References

- Shows: an Apple Podcasts link
  (`https://podcasts.apple.com/us/podcast/the-daily/id1200361736`),
  `id1200361736` or the number, or the show's feed URL. A web page that
  announces a feed works too.
- Episodes: `FEED_URL#GUID`, the link episode lists give (the GUID is the
  episode's id in the feed, percent-encoded where needed), or an Apple
  Podcasts episode link (`…/id1200361736?i=1000…`). Apple links are
  followed to the feed; Apple lists only a show's 200 newest episodes.
- `#N` works after any list.

## Commands

### search

`search WORDS [--filter shows|episodes|index]`: shows (by default) or
episodes from Apple's directory, or shows from Podcast Index (`index`,
needs a key). Apple returns at most 200 results, Podcast Index up to
1000 in one answer.

### hot

`--category shows|episodes|trending`: Apple's top 100 shows (by default)
or episodes of the storefront in `PODCAST_COUNTRY`, or the shows trending
on Podcast Index (`trending`, needs a key).

### read

A show: Apple's listing (author, genres, episode count, latest release)
completed from the feed (description, website, language), with the feed
in `extra.feed`. An episode: title, show notes, release time, its file in
`media` with the duration, episode and season numbers, the episode's web
page in `extra.link`, its Apple link in `extra.apple_url` when known.

### episodes

`episodes SHOW`: the show's episodes from its feed, the newest first;
`--since` / `--until` work on release times.

### download

`download EPISODE [-o DIR]` saves the episode file under the name the
publisher's server gives it (`dchha74_Mania_for_Subjugation_IV.mp3`);
names too generic to tell episodes apart (`media.mp3`, `default.mp3`)
are replaced by the episode title. An interrupted download continues
where it stopped (a `.part` file); a file of that name already in the
folder is kept, not fetched again. `--audio-only` keeps only the sound
of video episodes (needs ffmpeg).

## Limits

No account features: no subscriptions, play positions or ratings (Apple
offers no API for them). Apple's lookups see a show's 200 newest
episodes; feeds that publish only recent episodes show only those.
Private or paid feeds work only if their URL carries its own access
token.
