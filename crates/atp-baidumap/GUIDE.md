## Logging in

Baidu Map has no account login here: every command uses an AK (access
key) of the official Web Service API, and nothing works without one.

1. Register as a developer at https://lbsyun.baidu.com with a Baidu
   account and create an application at
   https://lbsyun.baidu.com/apiconsole/key, of type 「服务端」 (server). A
   browser or mobile AK does not work: Baidu answers `permission_denied`.
2. Choose how the AK is checked: an IP whitelist (enter this machine's
   public IP, or `0.0.0.0/0` for any) or SN verification, which gives an
   SK (secret key) that signs every request.
3. Run `atp bmap login --token AK`, or `--token AK:SK` for SN
   verification. `atp bmap whoami` checks the AK with an IP lookup and
   shows whether requests are signed (`extra.signed`).

`BAIDU_MAP_AK` (and `BAIDU_MAP_SK`) supply them for one run and win over
the saved ones; with `BAIDU_MAP_AK` set, only `BAIDU_MAP_SK` gives the SK.

An AK does not expire that ATP knows of. Failures name their cause: an AK
Baidu does not know or a disabled or deleted application is
`not_authenticated`; a wrong SK is
`signature_error`; a refused IP, a browser AK, or a service the AK lacks
(traffic must be switched on for AKs made before 2018-08-31) is
`permission_denied`.

Quotas are per service and per account tier (individual developers:
place search 100 a day, reverse geocoding 300, geocoding, routes, weather,
IP and static maps 5,000, traffic 2,000; 3 requests a second each);
going over gives `rate_limited`. Route matrices count every origin ×
destination pair. ATP waits 350 ms between requests.

## References

- Places: the uid (`435d7aea036e54355abbbcc8`), a map.baidu.com link
  carrying `uid=` or `poiShareUid=`, or `#N` after a list. Baidu asks
  for uids from a recent search rather than stored ones.
- Points: `LNG,LAT`, longitude first, in Baidu's BD-09 unless `--coord`
  says otherwise. With `--coord gcj02` or `wgs84` points travel to and from
  Baidu in GCJ-02 (`coord_type`, `ret_coordtype`), converted locally from
  and to WGS-84. `route`, `distance` and `weather` also take place names
  and geocode them.
- Regions: a city name or Baidu city code (`北京`, `131`).

## Commands

Results carry their points as `{lat, lng, system}`; `--coord wgs84|gcj02|bd09`
converts both what you give and what is printed.

### search

`search 咖啡 city:上海`: places in a city (nationwide without `city:`;
a keyword found in many cities then lists them). `--filter
food|hotel|parking|…` or `type:TAG` (a Baidu category such as `美食` or
`中餐厅`) sets the category. Pages hold 20 places, 150 per query.

### read

`read UID`: address, category, phone, rating, opening hours, price and
comment count.

### nearby

`nearby LNG,LAT [KEYWORD] --radius 1000 --filter food`: places inside the
circle, ranked by Baidu's relevance; Baidu needs a keyword or `--filter`
(the category name is then the keyword).

### tips

`tips KEYWORD [--city NAME] [--location LNG,LAT]`: up to 10 suggestions;
`--city` keeps them inside a city.

### geocode, regeo, ip

`geocode ADDRESS [--city NAME]` gives one point, with `precise`,
`confidence` (distance band of the error) and `comprehension`.
`regeo LNG,LAT [--pois]`; locations outside China need a permission from
Baidu. `ip [IP]` gives the city and its centre (IPv6 needs a permission).

### route

`route FROM TO --type driving|walking|cycling|ebike|transit`: distance,
time, tolls, traffic lights and steps (driving steps name the road; Baidu
gives them no instructions), with an encoded polyline. Driving asks for
up to three routes. `--strategy` picks driving (`shortest`, `no-highway`,
`highway`, `avoid-jam`, `least-fee`, `fastest`) or transit
(`fewest-transfers`, `least-walk`, `no-subway`, `fastest`, `subway`)
plans. `--city` helps place names.

### distance

`distance A B --to C --to D --mode straight|driving|walking|cycling|ebike`:
every origin to every destination, at most 100 pairs driving and 50
otherwise (walking pairs at most 200 km apart). `straight` is measured by
ATP.

### weather

`weather CITY [--forecast]`: the weather now (temperature, feels-like,
humidity, wind, AQI), or the forecast for the next days. CITY is a
district adcode, `LNG,LAT`, or a name, geocoded first.

### traffic

`traffic ROAD --city CITY`, `traffic --rect 'LNG,LAT;LNG,LAT'` (diagonal
at most 2 km) or `traffic --around LNG,LAT --radius 800` (at most 1000):
the overall status (smooth, slow, congested, severe), Baidu's description
and the congested sections of each road with speed, length and trend.

### staticmap

`staticmap LNG,LAT --zoom 15 --size 600*400 [--scale 2] -o map.png`:
sizes up to 1024*1024 (512*512 at scale 2), zoom 3–18; no traffic layer.

## Limits

Baidu's services cover mainland China; abroad most return nothing, and
global reverse geocoding needs a permission. BD-09 points Baidu never
sees (`--coord bd09` on another platform) are converted with the public
BD-09 formulas, whose inverse is approximate (in ATP's checks well under
a metre).
