## Logging in

Amap has no account login here: every command uses a key of the official
Web Service API, and nothing works without one.

1. Create a key at https://console.amap.com/dev/key/app (「创建新应用」 →
   「添加 Key」 → service platform 「Web服务」). A JS API or mobile key does
   not work: Amap answers `permission_denied`.
2. If you turn on 数字签名 for the key, keep its private key too: every
   request is then signed with it.
3. Run `atp amap login --token KEY`, or `--token KEY:SIGN_SECRET` for a
   signed key. `atp amap whoami` checks the key with an IP lookup and shows
   whether requests are signed (`extra.signed`).

`AMAP_KEY` (and `AMAP_SIGN_SECRET`) supply them for one run and win over
the saved ones; with `AMAP_KEY` set, only `AMAP_SIGN_SECRET` gives the
secret.

A key does not expire that ATP knows of. Failures name their cause: a key
Amap does not accept is `not_authenticated`, a wrong signing secret
`signature_error`. An IP whitelist on the key gives `permission_denied`
from other machines, and so does a service the key has no permission for.

Free keys have daily quotas and a per-second limit per service; going over
them gives `rate_limited` (wait a moment, or raise the key's quota in the
console). ATP waits 150 ms between requests.

## References

- Places (POIs): the id (`B000A83M61`), a place link
  (`https://www.amap.com/place/B000A83M61`, `…?poiid=…`), or `#N` after a
  list.
- Points: `LNG,LAT` in Amap's GCJ-02 coordinates (`116.481,39.990`). Use
  `convert` for GPS (WGS-84), Baidu or Mapbar ones. `route` and `distance`
  also take place names and geocode them.
- Regions: a city or district name (`北京`, `朝阳区`) or its 6-digit adcode.

## Commands

Place lists hold at most 25 places a page and 200 per query.

### search

`search 咖啡 city:上海`: places matching a keyword. `city:NAME` keeps
results inside a city; `type:CODE` (Amap type codes, `|` between several)
or `--filter food|hotel|parking|…` limits the category.

### read, download

`read ID`: details (phone, rating, cost, opening hours, entrances, indoor
floor, sub-places) and photo links. `download ID` saves the photos.

### nearby

`nearby LNG,LAT [KEYWORD] --radius 1000 --filter food`: places nearest
first, within at most 50 km. `--filter` takes a category name or an Amap
type code.

### tips

`tips KEYWORD [--city] [--location]`: suggestions as you type; `--city`
keeps them inside a city, `--location` ranks those nearby first.

### geocode, regeo, ip

`geocode ADDRESS [--city]`, `regeo LNG,LAT [--pois]`, `ip [IP]` (your own
address by default; mainland China only).

### district

`district NAME [--depth 0-3]`: districts and the levels below them, with
adcodes and centres; `中国` lists the provinces.

### convert

`convert 'LNG,LAT|LNG,LAT' --from gps|baidu|mapbar`.

### route

`route FROM TO --type driving|walking|cycling|ebike|transit`: distance,
time, tolls, traffic lights and steps; transit gives each plan's legs.
`--strategy` picks driving (`avoid-jam`, `highway`, `no-highway`,
`least-fee`, `big-road`, `fastest`, `no-toll`) or transit (`cheapest`,
`fewest-transfers`, `least-walk`, `comfortable`, `no-subway`, `subway`,
`fastest`) plans. `--city` helps place names.

### distance

`distance A B C --to D --mode straight|driving|walking`: up to 100 places
to one destination.

### weather

`weather CITY [--forecast]`: the weather now, or a four-day forecast.

### traffic

`traffic ROAD --city CITY`, `traffic --rect 'LNG,LAT;LNG,LAT'` or
`traffic --around LNG,LAT --radius 1000`: road status (smooth, slow,
congested, severe).

### staticmap

`staticmap LNG,LAT --zoom 15 --size 600*400 [--scale 2] [--traffic] -o
map.png`.

## Limits

Amap's services cover mainland China; abroad most of them return nothing.
