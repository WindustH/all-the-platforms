Hacker News through its two open APIs: the official Firebase API
(front-page lists, items, users) and Algolia's HN Search API (search,
whole comment threads, a user's stories and comments). Everything is
read-only.

## Logging in

There is nothing to log in to: every command works without an account,
and ATP has no way to vote, comment or submit. `atp hn status` therefore
always reports "not logged in".

The Firebase API documents no rate limit; Algolia allows an IP 10,000
requests an hour. ATP does not pace Hacker News requests, but fetches
items at most eight at a time. No environment variables are read.

## References

- Items (stories, comments, jobs, polls): a link
  (`https://news.ycombinator.com/item?id=8863`), `item?id=8863`, or the id.
- Users: a profile link (`https://news.ycombinator.com/user?id=pg`),
  `user?id=pg`, `@pg` or `pg`.
- `#N` works after any list.

## Commands

### hot

`--category top|new|best|ask|show|job`: the lists of the front page (top
stories by default), up to 500 stories each (200 for ask, show and job).
Each page loads the list again, then its items.

### search

Algolia search. `--filter story|comment|ask_hn|show_hn|job|poll|front_page|all`
(stories by default; `all` finds stories and comments), `--sort
relevance|date` (`date`: newest first). Comments come back as posts of
kind `comment`, titled with their story.

### read

One item: a story with its link in `extra.link` and the site in
`extra.domain`, the text of Ask HN posts, jobs, and comments (with
`extra.parent_id`). A poll lists its options with their points.

### comments

The whole thread in two requests: every comment and reply from Algolia,
the top level in Hacker News' own order from Firebase. Replies under a
comment come oldest first. `--sort top|new|old` (`new` and `old` reorder
every level by time). Top-level comments too fresh for Algolia are loaded
from Firebase, and their replies follow with `--replies`; fresh replies
deeper in the thread show once Algolia has them. Deleted comments that
still have replies show as `[deleted]`; flagged (dead) ones are left out.

### replies

Replies under one comment, from the thread, or from Firebase for a
comment Algolia does not have yet.

### user

Karma, the "about" text, when the account was made, and how many items it
submitted (`stats.posts`).

### user-posts

The user's stories, polls and jobs, newest first (through Algolia).

### threads

`threads USER`: the comments a user wrote, newest first, each titled with
its story.

## Limits

No login, so no voting, commenting, submitting, favorites or hiding.
Algolia indexes new items after a delay, so the newest stories and
comments may be missing from `search`, `user-posts` and `threads` for a
while. Dead and deleted items are hidden in lists.
