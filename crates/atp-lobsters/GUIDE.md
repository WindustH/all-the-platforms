Lobsters (lobste.rs), the link-sharing community about computing, through
the JSON versions of its public pages. Everything is read-only.

## Logging in

There is nothing to log in to: every command works without an account,
and ATP has no way to vote, comment, flag or submit. `atp lobsters
status` therefore always reports "not logged in".

Lobsters publishes no rate limit for its JSON pages. ATP spaces requests
at least half a second apart; one listing page is one request. Requests
name ATP as their client (`User-Agent`), because the HTML pages, which
`search` reads, put web browsers through a proof-of-work check (Anubis).
No environment variables are read.

## References

- Stories: a link (`https://lobste.rs/s/7xg9rm/introducing_firefox`),
  `s/<id>`, or the short id (`7xg9rm`).
- Comments: a link (`https://lobste.rs/c/fd69kw`), `c/<id>`, or a story
  link ending in `#c_<id>`.
- Users: a profile link (`https://lobste.rs/~name`), `~name`, `@name`, or
  the name.
- `#N` works after any list.

## Commands

### hot

`--category hottest|newest|active`: the front page (by default), the
newest stories, or the stories with the most recent discussion. 25 stories
a page.

### search

Stories whose title, text or link match. `--sort relevance|newest|score`.
Lobsters' search page has no JSON version, so results come from its HTML:
title, link, score, tags, submitter, time and comment count, without the
story text (`read` has it).

### read

A story: title, its text, the linked page in `extra.link` (with
`extra.domain`), tags, score and comment count. A comment reference reads
the comment.

### comments

The story's whole discussion from one request, as threads in the page's
order; every reply is included already. Removed comments show as
`[deleted]` with `extra.removed`.

### replies

Replies under one comment, from the same thread.

### user, user-posts

Karma, the profile text, avatar, when they joined and who invited them;
`user-posts` lists the stories they submitted, newest first.

### tag, tags

`tag NAME` lists the stories with a tag; `tags` lists every active tag
with its description and category.

## Limits

No login, so no voting, commenting, submitting, saving or hiding. Search
finds stories only (not comments).
