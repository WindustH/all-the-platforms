## Logging in

Google Maps has no account login here: every command calls the official
Google Maps Platform APIs with your own API key, and nothing works without
one.

1. In the Google Cloud console, create a project and turn on billing for
   it (Google requires a billing account even within the free usage).
2. Enable the APIs you want under APIs & Services → Library: Places API
   (New), Geocoding API, Routes API, Time Zone API, Elevation API, Maps
   Static API.
3. Create an API key under Google Maps Platform → Credentials
   (https://console.cloud.google.com/google/maps-apis/credentials). Restrict
   it to these APIs; an IP-address restriction works too, a website
   (referrer) restriction does not.
4. Run `atp gmap login --token KEY`. `atp gmap whoami` checks the key with
   an IDs-only text search, which Google does not bill, and reports in
   `extra.places_api` whether Places API (New) answers.

`GOOGLE_MAPS_API_KEY` supplies the key for one run and wins over the saved
one. `GOOGLE_MAPS_LANGUAGE` (`en`, `zh-CN`, `ja` …) sets the language of
names, addresses and instructions; without it Places and Routes answer in
English or the place's own language, the web services (geocoding, time
zones) in the language ATP's requests prefer (Chinese, then English).

A key does not expire that ATP knows of; it stops working when it is
deleted or its project's billing ends. A key Google does not accept is
`not_authenticated`; an API that is not enabled or not allowed by the key's
restrictions, a refused IP address or missing billing is
`permission_denied` with the reason; a used-up quota is `rate_limited`.
ATP waits 100 ms between requests. Every command is billed to the key's
project; the Billing section below lists what each one costs.

## References

- Places: the place id (`ChIJN1t_tDeuEmsRUsoyG83frY4`), `places/ID`, a
  link carrying `place_id:ID` or `query_place_id=ID`, or `#N` after a list.
- Points: `LNG,LAT`, longitude first, in WGS-84 unless `--coord` says
  otherwise. Google's map data in mainland China is GCJ-02 (Hong Kong,
  Macao and Taiwan are not), so ATP sends points there in GCJ-02 and
  converts Google's answers there back to the `--coord` system. `route`
  and `distance` also take addresses. West of Greenwich points start with
  `-`; the lists of `distance` and `elevation` need `--` before them
  (`atp gmap elevation -- -122.084,37.422 -122.42,37.77`).

## Commands

Results carry their points as `{lat, lng, system}`; `--coord wgs84|gcj02|bd09`
converts both what you give and what is printed.

### search

`search 'ramen' city:Tokyo`: Text Search; `city:NAME` is added to the
text, `--filter cafe|hotel|parking|…` or `type:TYPE` (a Google place type)
limits the category. Pages hold up to 20 places, 60 per query.

### read, download

`read ID`: address, location, category, phone, rating, opening hours,
website and photos. `download ID` asks Google for a short-lived link of
each photo (up to 10) and saves them; photos need their authors credited
when shown.

### nearby

`nearby LNG,LAT --radius 800 --filter cafe`: Nearby Search, nearest first,
at most 20 places (Google pages no further). With a keyword it is a Text
Search biased to the circle, ranked by relevance, so places just outside
can appear. `distance_m` is measured by ATP from the point.

### tips

`tips TEXT [--location LNG,LAT] [--city NAME]`: up to five suggestions:
places (`kind: tip`, their ids work with `read`) and queries (`kind:
query`). `--location` ranks nearby ones first and measures their distance;
`--city` is added to the text. Suggestions carry no points.

### geocode, regeo

`geocode ADDRESS [--city NAME]` lists matches with their place ids;
`--city` biases them. `regeo LNG,LAT` gives the most precise address;
`--pois` adds the places within 100 m (Nearby Search Pro).

### route

`route FROM TO --type driving|walking|cycling|ebike|transit`: up to three
routes with distance, time, steps and an encoded polyline. `ebike` is
Google's two-wheeler (motorbikes, scooters). `--strategy`: driving and
ebike `avoid-jam` (live traffic), `fastest` (traffic-aware optimal),
`no-toll`, `least-fee`, `no-highway`, `no-ferry`; transit `least-walk`,
`fewest-transfers`, `subway`, `no-subway`, `bus`, `rail`. Google marks
walking, cycling and two-wheeler routes as beta. `--city` is added to
addresses.

### distance

`distance A B --to C --to D --mode straight|driving|walking|cycling|ebike`:
every origin to every destination (at most 625 pairs, 50 of them given as
addresses). `straight` is measured by ATP, geocoding addresses first.

### timezone

`timezone LNG,LAT [--at 2026-12-01]`: the zone id and name, the offset
from UTC in seconds at that moment (daylight saving included) and the
local time.

### elevation

`elevation LNG,LAT LNG,LAT …`: metres above sea level (negative below)
and the resolution of the data, up to 512 points.

### staticmap

`staticmap LNG,LAT --zoom 15 --size 600*400 [--scale 2] -o map.png`: a
roadmap with a marker; sizes up to 640*640 (1280*1280 pixels at scale 2).
Google has no traffic layer here.

## Billing

Every request is billed to the key's project at the SKU its fields and
options trigger; each SKU has a monthly free allowance (global prices
since March 2025, USD per 1,000 after it):

| Commands | SKU | Free per month | Price |
| --- | --- | --- | --- |
| `search`, `nearby` with a keyword | Text Search Pro | 5,000 | $32 |
| `nearby` without a keyword, `regeo --pois` | Nearby Search Pro | 5,000 | $32 |
| `read` | Place Details Enterprise | 1,000 | $20 |
| `download` | Place Details IDs Only + Place Photos per photo | unlimited + 1,000 | free + $7 |
| `tips` | Autocomplete Requests | 10,000 | $2.83 |
| `geocode`, `regeo`, names in `distance --mode straight` | Geocoding | 10,000 | $5 |
| `route` | Compute Routes Essentials | 10,000 | $5 |
| `route --strategy avoid-jam/fastest` | Compute Routes Pro | 5,000 | $10 |
| `route --type ebike` | Compute Routes Enterprise | 1,000 | $15 |
| `distance` (per origin × destination) | Compute Route Matrix, same tiers | as routes | as routes |
| `timezone` | Time Zone | 10,000 | $5 |
| `elevation` | Elevation | 5,000 | $5 |
| `staticmap` | Static Maps | 10,000 | $2 |
| `whoami` | Text Search IDs Only | unlimited | free |

ATP sends field masks with only the fields it prints, so lists stay at
Pro and details at Enterprise (no Atmosphere fields such as reviews).
Routes never ask for tolls (they would bill Enterprise). Set budgets and
quota caps in the Cloud console; Google may change prices.

## Limits

- Places, Routes and the web services are reachable only where Google is;
  mainland Chinese networks block them.
- Text Search returns at most 60 places per query, Nearby Search 20.
- Place ids can change; Google suggests refreshing stored ones.
- Mainland China is told apart from Hong Kong, Macao, Taiwan and the
  neighbouring countries on a 0.25° grid, so points within a few
  kilometres of a land border may be treated as either.
