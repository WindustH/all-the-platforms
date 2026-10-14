## Getting a key

Amap has no account login here: every command uses a key of the official
Web Service API. Create one at https://console.amap.com/dev/key/app
(「创建新应用」 → 「添加 Key」 → service platform 「Web服务」; a JS API or
mobile key does not work). If you turn on 数字签名 for the key, keep its
private key too.

- `atp amap login --token KEY` or `--token KEY:SIGN_SECRET`; `atp amap
  whoami` checks it.
- `AMAP_KEY` (and `AMAP_SIGN_SECRET`) supply them for one run and win over
  the saved ones.

Free keys have daily quotas and a per-second limit per service; going over
them gives `rate_limited`. An IP whitelist on the key gives
`permission_denied` from other machines.

## References

- Places (POIs): the id (`B000A83M61`), a place link
  (`https://www.amap.com/place/B000A83M61`, `…?poiid=…`), or `#N` after a
  list.
- Points: `LNG,LAT` in Amap's GCJ-02 coordinates (`116.481,39.990`). Use
  `convert` for GPS (WGS-84), Baidu or Mapbar ones. `route` and `distance`
  also take place names and geocode them.
- Regions: a city or district name (`北京`, `朝阳区`) or its 6-digit adcode.

## Places

- `search 咖啡 city:上海`: places matching a keyword; `city:NAME` keeps
  results inside a city, `type:CODE` (Amap type codes, `|` between several)
  or `--filter food|hotel|parking|…` limits the category.
- `read ID`: details (phone, rating, cost, opening hours, entrances, indoor
  floor, sub-places) and photo links; `download ID` saves the photos.
- `nearby LNG,LAT [KEYWORD] --radius 1000 --filter food`: nearest first.
- `tips KEYWORD [--city] [--location]`: suggestions as you type.

At most 25 places per page and 200 per query.

## Addresses and areas

- `geocode ADDRESS [--city]`, `regeo LNG,LAT [--pois]`, `ip [IP]`.
- `district NAME [--depth 0-3]`: districts and the levels below them, with
  adcodes and centres.
- `convert 'LNG,LAT|LNG,LAT' --from gps|baidu|mapbar`.

## Routes

- `route FROM TO --type driving|walking|cycling|ebike|transit`: distance,
  time, tolls, traffic lights and steps; transit gives each plan's legs.
  `--strategy` picks driving (`avoid-jam`, `highway`, `no-highway`,
  `least-fee`, `big-road`, `fastest`, `no-toll`) or transit (`cheapest`,
  `fewest-transfers`, `least-walk`, `comfortable`, `no-subway`, `subway`,
  `fastest`) plans. `--city` helps place names.
- `distance A B C --to D --mode straight|driving|walking`.

## Weather, traffic and maps

- `weather CITY [--forecast]`.
- `traffic ROAD --city CITY`, `traffic --rect 'LNG,LAT;LNG,LAT'` or
  `traffic --around LNG,LAT --radius 1000`: road status (smooth, slow,
  congested, severe).
- `staticmap LNG,LAT --zoom 15 --size 600*400 [--scale 2] [--traffic] -o
  map.png`.

Amap's services cover mainland China; abroad most of them return nothing.
