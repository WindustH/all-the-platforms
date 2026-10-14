## Use with care

Ctrip's terms (携程平台用户服务协议) forbid collecting the platform's data
with crawlers and logging in with software, and its robots.txt keeps
agents out of `/restapi/`. atp reads only what you ask for, one request at
a time, at least 2.5 seconds apart, with no "fetch everything" defaults.
Keep it personal and low-volume. Busy addresses get Ctrip's firewall
(HTTP 430, `ip_blocked`) for an hour or more; atp stops there and does
not retry.

## Logging in

Everything except `rooms` works without an account. For `rooms`, log in
on www.ctrip.com in your browser and run `atp ctrip login --browser`, or
`--cookie` with `cticket` (plus `GUID`, `DUID`, `_RF1`, `UBT_VID` as the
browser has them). `atp ctrip whoami` checks the session with Ctrip's
member service. atp offers no QR login (Ctrip's needs a browser
fingerprint).

## References

- Sights: `sight:49894`, a `you.ctrip.com/sight/<city><district>/<id>.html`
  link, or atp's own `m.ctrip.com/webapp/you/gspoi/sight/<district>/<id>.html?poiId=<poi>`
  (the `url` of listed sights; it carries the POI id reviews want).
  `poi:82922` names a sight by its POI id. `sight` also takes the bare id.
- Hotels: `hotel:445394`, `hotels.ctrip.com/hotels/detail/?hotelid=<id>` or
  `hotels.ctrip.com/hotel/<id>.html`; `hotel` and `rooms` take the bare id.
- Notes: `note:15103061` or `m.ctrip.com/webapp/you/community/detail?articleId=<id>`.
- Travel notes (游记, archived since 2025-11-30): `you.ctrip.com/travels/<city><district>/<id>.html`.
- Users: the `clientAuth` (32 or 64 hex digits) or a
  `m.ctrip.com/webapp/you/community/user?clientAuth=<id>` link.
- Rankings: the id or `m.ctrip.com/webapp/xranking/cranking/crankingCity/<id>.html`.
- Cities: a name (`杭州`) or an id. Sights, rankings and the community
  number cities by district id (杭州 14), hotels by city id (杭州 17);
  `cities NAME` shows both, and names are looked up once and remembered.
  `hot` and `feed --type city` use `CTRIP_CITY` (a name or district id),
  Shanghai without it.

## Reading

- `search WORDS`: places of every kind (sights, hotels, restaurants,
  districts, tours), 10 a page. `suggest WORDS [--hotels]`: the booking
  forms' suggestions with city ids and coordinates.
- `sights CITY [--sort default|rating|hot|sales] [--5a] [--free]`: score,
  review count, heat, level, area and the ticket price from (`price`,
  label `门票起`; free sights `免费`). Coordinates (`extra.location`,
  `lng,lat`) are Baidu's BD09.
- `read SIGHT`: score and sub-scores (景色/趣味/性价比), address, hours,
  phones, ranking, introduction and photos. `sight SIGHT` adds the
  coordinates, ticket notes, opening status and facilities.
- `comments SIGHT [--sort default|new|good|bad]`: reviews with their
  score, sub-scores, trip type, pictures and the sight's reply. Ctrip lists
  at most about 3000 reviews per order.
- `hotels CITY [--checkin --checkout]`: the city's hotel list as its first
  screen shows it (about 13 hotels): stars, score, sub-scores, review
  count, area, address and coordinates. Visitors see no prices there.
- `read HOTEL` / `hotel HOTEL`: score and sub-scores (卫生/设施/环境/服务),
  review count, address, check-in and check-out, facilities, facts (opened,
  renovated, rooms), phone, description, photos and video.
- `comments HOTEL [--sort default|new|good|bad]`: reviews with rating, room,
  trip type, stay month, pictures and the hotel's answer. Hotels rate
  overall only.
- `rooms HOTEL [--checkin --checkout --adults]` (login): room plans with
  price (`price`, the crossed-out price in `original`), bed, size, meals,
  cancellation and rooms left. Ctrip may stop it with its risk control
  (`verification_required`).
- `rankings CITY` and `ranking ID`: a city's rankings and the entries its
  page shows first. `hot [--category must-see|night|family|experience]`:
  a ranking of `CTRIP_CITY`.
- `trains FROM TO [--date]`: the day's trains, each with departure and
  arrival, duration, seats with fares and tickets left (`余N`; 99 means
  plenty), the lowest fare in `price`. The post's time is the departure.
- `transfers FROM TO [--date]`: the day's plans with one change (train
  and train, sometimes a flight and a train), in Ctrip's order: both legs
  with times, the wait, seats and fares (`extra.legs`), the plan's lowest
  total in `price`.
- `fares FROM TO [--date] [--direct]`: the lowest one-way flight fare of
  each day from `--date` on (about a year ahead), with taxes and fees in
  the price's label and the cheapest days marked `extra.lowest`. Cities
  are names (`上海`) or airport-city codes (`SHA`); the post's `url` is
  the day's flight list on flights.ctrip.com.
- `read NOTE`, `comments NOTE`, `feed [--type recommend|city]`,
  `user USER`, `user-posts USER` (notes, reviews and answers):
  the community. `download NOTE|SIGHT|HOTEL` saves the full-size photos
  and videos.

## Signing and risk control

Nothing is signed. atp sends what the H5 pages send (a visitor id, a trace
id and the request `head`) from the site each service expects. It does not
make Ctrip's anti-spider token (`phantom-token`), the `w-payload-source`
signature or RMS browser fingerprints, so the services that want them are
left out. When Ctrip shows a captcha or a check, atp ends with
`verification_required` and the page; on a terminal it opens it in your
Chrome, and retries after you pass it.

## Limits

No flight lists: Ctrip's gateway refuses its H5 flight search to web
clients, its streamed search sits behind the firewall's bot check, and
flights.ctrip.com wants a login and a browser fingerprint. No hotel
prices for visitors, no hotel list beyond its first screen (the list
service wants the anti-spider token) and no ranking beyond its first
entries.
No bookings, orders, payments, coupons, favorites, likes, comments, notes
or private messages. atp does not solve captchas.
