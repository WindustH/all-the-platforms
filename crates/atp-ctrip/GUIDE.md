## Logging in

Everything except `rooms` and `whoami` works without an account.

For `rooms`, log in on www.ctrip.com in your browser and run `atp ctrip
login --browser` (`atp ctrip login` with no flag does the same). Or copy
the `Cookie` header of a request to www.ctrip.com and run `atp ctrip
login --cookie '...'`: it needs `cticket`; include `GUID`, `DUID`,
`_RF1` and `UBT_VID` as the browser has them. `ATP_CTRIP_COOKIE`
supplies such a header for one run and is never saved. ATP offers no QR
login (Ctrip's needs a browser fingerprint).

`atp ctrip whoami` checks the session with Ctrip's member service and
shows the account's name, grade and SVIP status. How long a session
lasts is not known; when it ends, `rooms` fails with
`not_authenticated` ("Ctrip wants a login for this", or "Ctrip shows
hotel prices after a login (4030)").

### Use with care

Ctrip's terms (携程平台用户服务协议) forbid collecting the platform's data
with crawlers and logging in with software, and its robots.txt keeps
agents out of `/restapi/`. ATP reads only what you ask for, one request
at a time, at least 2.5 seconds apart, with no "fetch everything"
defaults. Keep it personal and low-volume.

- Busy addresses get Ctrip's firewall (HTTP 430, `ip_blocked`) for an
  hour or more; ATP stops there and does not retry.
- When Ctrip shows a captcha or a check, or its gateway refuses a
  request (HTTP 403), ATP ends with `verification_required` and the
  page; on a terminal it opens it in a browser and retries after you
  pass it (see `atp guide login`).
- A call Ctrip refuses to web clients (11010) gives `permission_denied`.

## References

- Sights: `sight:49894`, a `you.ctrip.com/sight/<city><district>/<id>.html`
  link, or ATP's own `m.ctrip.com/webapp/you/gspoi/sight/<district>/<id>.html?poiId=<poi>`
  (the `url` of listed sights; it carries the POI id reviews want).
  `poi:82922` names a sight by its POI id. `sight` also takes the bare id.
- Hotels: `hotel:445394`, `hotels.ctrip.com/hotels/detail/?hotelid=<id>` or
  `hotels.ctrip.com/hotel/<id>.html`; `hotel` and `rooms` take the bare id.
- Notes: `note:15103061` or `m.ctrip.com/webapp/you/community/detail?articleId=<id>`.
- Travel notes (游记, archived since 2025-11-30): `you.ctrip.com/travels/<city><district>/<id>.html`.
- Users: the `clientAuth` (32 or 64 hex digits) or a
  `m.ctrip.com/webapp/you/community/user?clientAuth=<id>` link.
- Rankings: `ranking:<id>`, the id, or
  `m.ctrip.com/webapp/xranking/cranking/crankingCity/<id>.html`.
- Cities: a name (`杭州`) or an id. Sights, rankings and the community
  number cities by district id (杭州 14), hotels by city id (杭州 17);
  `cities NAME` shows both, and names are looked up once and remembered.
  `hot` and `feed --type city` use `CTRIP_CITY` (a name or district id),
  Shanghai without it.
- Dates: `YYYY-MM-DD`; days default to today in Beijing.

## Commands

Sights and hotels are posts (`kind` `sight` / `hotel`, prices in
`price`); their reviews are comments; community notes are posts and
their authors users; rankings are collections; trains, transfer plans
and the days of a fare calendar are posts with fares.

### search

`search WORDS`: places of every kind (sights, hotels, restaurants,
districts, tours), 10 a page, with their web links.

### suggest

`suggest WORDS [--hotels]`: the booking forms' suggestions with city ids
and coordinates; `--hotels` gives the hotel search's places (hotels,
business areas).

### cities

`cities NAME`: the places of a name with both the district id (sights,
rankings, the community) and the hotel city id.

### sights

`sights CITY [--sort default|rating|hot|sales] [--5a] [--free]`: score,
review count, heat, level, area and the ticket price from (`price`,
label `门票起`; free sights `免费`). Coordinates (`extra.location`,
`lng,lat`) are Baidu's BD09.

### read

- A sight: score and sub-scores (景色/趣味/性价比), address, hours,
  phones, ranking, introduction and photos.
- A hotel: score and sub-scores (卫生/设施/环境/服务), review count,
  address, check-in and check-out, facilities, facts (opened, renovated,
  rooms), phone, description, photos and video.
- A note, or an archived travel note read from its page.

A ranking is a list: use `ranking`.

### sight

`sight SIGHT`: what `read` shows, plus the coordinates, ticket notes,
opening status and facilities.

### hotel

`hotel HOTEL`: the same as `read HOTEL`; it also takes the bare id.

### comments

- `comments SIGHT [--sort default|new|good|bad]`: reviews with their
  score, sub-scores, trip type, pictures and the sight's reply. Ctrip
  lists at most about 3000 reviews per order.
- `comments HOTEL [--sort ...]`: reviews with rating, room, trip type,
  stay month, pictures and the hotel's answer. Hotels rate overall only.
- `comments NOTE`: the note's comments. Archived travel notes have none
  ATP can read.

### hotels

`hotels CITY [--checkin --checkout]`: the city's hotel list as its first
screen shows it (about 13 hotels): stars, score, sub-scores, review
count, area, address and coordinates. Visitors see no prices there.

### rooms

`rooms HOTEL [--checkin --checkout --adults]` needs a login: room plans
with price (`price`, the crossed-out price in `original`), bed, size,
meals, cancellation and rooms left, cheapest first. Check-out defaults
to the day after check-in. Ctrip may stop it with its hotel risk
control (`verification_required`).

### rankings, ranking

`rankings CITY`: a city's rankings (必打卡, 夜游, 亲子 …). `ranking ID`:
the entries its page shows first.

### hot

`hot [--category must-see|night|family|experience]`: the matching
ranking of `CTRIP_CITY` (Shanghai without it); without a category
必打卡, else the city's first ranking. A city without that ranking gives `not_found` with the names
it has.

### trains

`trains FROM TO [--date]`: the day's trains, each with departure and
arrival, duration, seats with fares and tickets left (`余N`; 99 means
plenty), the lowest fare in `price`. The post's time is the departure.
FROM and TO are stations or cities (`上海`, `上海虹桥`).

### transfers

`transfers FROM TO [--date]`: the day's plans with one change (train
and train, sometimes a flight and a train), in Ctrip's order: both legs
with times, the wait, seats and fares (`extra.legs`), the plan's lowest
total in `price`.

### fares

`fares FROM TO [--date] [--direct]`: the lowest one-way flight fare of
each day from `--date` on (about a year ahead), with taxes and fees in
the price's label and the cheapest days marked `extra.lowest`. Cities
are names (`上海`) or airport-city codes (`SHA`); the post's `url` is
the day's flight list on flights.ctrip.com.

### feed

`feed [--type recommend|city]`: the community square (推荐) or the notes
of `CTRIP_CITY` (同城), 10 a page.

### user, user-posts

`user USER`: a community profile. `user-posts USER`: the user's notes,
reviews and answers, 10 a page.

### download

`download NOTE|SIGHT|HOTEL` saves the full-size photos and videos.

## Signing

Nothing is signed. ATP sends what the H5 pages send (a visitor id, a
trace id and the request `head`) from the site each service expects.
The visitor id is the session's `GUID` cookie, else one ATP makes once
and keeps. ATP does not make Ctrip's anti-spider token
(`phantom-token`), the `w-payload-source` signature or RMS browser
fingerprints, so the services that want them are left out.

## Limits

No flight lists: Ctrip's gateway refuses its H5 flight search to web
clients, its streamed search sits behind the firewall's bot check, and
flights.ctrip.com wants a login and a browser fingerprint. No hotel
prices for visitors, no hotel list beyond its first screen (the list
service wants the anti-spider token) and no ranking beyond its first
entries. No bookings, orders, payments, coupons, favorites, likes,
comments, notes or private messages. ATP does not solve captchas.
