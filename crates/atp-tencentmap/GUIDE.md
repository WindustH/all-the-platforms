## Logging in

Tencent Map has no account login here: every command uses a key of the
official WebService API, and nothing works without one.

1. Register at https://lbs.qq.com and create a key under 应用管理
   (https://lbs.qq.com/dev/console/application/mine).
2. In the key's settings, tick 「WebServiceAPI」 and choose how calls are
   allowed: authorized IPs, or 签名校验 (signature check), which gives a
   secret key (SK) that signs every request. Give the key quota for the
   services you use (quota is shared by all keys of an account); a new key
   without it fails with `permission_denied`.
3. Run `atp qqmap login --token KEY`, or `--token KEY:SK` with signature
   checks. `atp qqmap whoami` checks the key with an IP lookup and shows
   whether requests are signed (`extra.signed`).

`TENCENT_MAP_KEY` (and `TENCENT_MAP_SK`) supply them for one run and win
over the saved ones; with `TENCENT_MAP_KEY` set, only `TENCENT_MAP_SK`
gives the SK.

A key does not expire that ATP knows of. Failures name their cause: an
unknown or malformed key is `not_authenticated`, a wrong SK
`signature_error`, a refused IP or domain, WebServiceAPI turned off or a
service without quota `permission_denied`.

Individual developers get 200 place searches, details and
recommendations a day and 6,000 calls a day of every other service
(each origin × destination pair of a distance matrix counts as one), at
most 5 a second; going over gives `rate_limited`. ATP waits 220 ms between
requests.

## References

- Places: the id (`3629720141162880123`), a map.qq.com link carrying
  `uid=`, or `#N` after a list.
- Points: `LNG,LAT`, longitude first, in Tencent's GCJ-02 unless `--coord`
  says otherwise. `route`, `distance` and `weather` also take place names
  and geocode them.
- Regions: a city or district name (`北京`, `海淀区`) or its 6-digit adcode.

## Commands

Results carry their points as `{lat, lng, system}`; `--coord wgs84|gcj02|bd09`
converts both what you give and what is printed.

### search

`search 咖啡 city:上海`: places in a city (nationwide without `city:`; a
keyword found in many cities then lists them). `--filter
food|hotel|parking|…` or `type:NAME` (a Tencent category name or code such
as `美食`) sets the category. Pages hold 20 places, 200 per query.

### read

`read ID`: name, address, category, phone and district.

### nearby

`nearby LNG,LAT [KEYWORD] --radius 800 --filter food`: places nearest
first within at most 1000 m. Without a keyword Tencent recommends the
places around the point (radius 200–1000 m).

### tips

`tips KEYWORD [--city NAME] [--location LNG,LAT]`: suggestions; `--city`
keeps them inside a city, `--location` ranks nearby ones first and
measures their distance.

### geocode, regeo, ip

`geocode ADDRESS [--city NAME]` gives one point with its `level` and
`reliability` (1–10; 7 and above is fairly sure); `--city` is put in front
of the address, which Tencent wants to contain one. `regeo LNG,LAT
[--pois]`; `ip [IP]` (IPv4 or IPv6) gives the district and its centre.

### district

`district NAME [--depth 0-2]`: districts and the levels below them, with
adcodes and centres; `中国` lists the provinces. Tencent's list has three
levels: provinces, cities and districts.

### route

`route FROM TO --type driving|walking|cycling|ebike|transit`: distance,
time, tolls, traffic lights, instructions and the route's polyline;
driving gives up to three routes, transit each plan's walks and rides with
their fares. `--strategy` picks driving (`avoid-jam`, `least-fee`,
`highway`, `no-highway`, `big-road`) or transit (`fastest`,
`fewest-transfers`, `least-walk`, `no-subway`, `subway`) plans. `--city`
helps place names.

### distance

`distance A B --to C --to D --mode straight|driving|walking|cycling|ebike`:
every origin to every destination (at most 625 pairs, 50 a side).

### weather

`weather CITY [--forecast]`: the weather now, or the forecast for the next
seven days. CITY is a district name or adcode, or `LNG,LAT`.

### staticmap

`staticmap LNG,LAT --zoom 15 --size 600*400 [--scale 2] -o map.png`:
sizes from 50*50 up to 1680*1200, zoom 4–18; no traffic layer.

## Limits

Tencent's services cover China; abroad most return nothing. Tencent
notes that its Taiwan points are WGS-84, while ATP treats every Tencent
point as GCJ-02, so `--coord` conversions there are off by a few hundred
metres. There is no live-traffic service.
