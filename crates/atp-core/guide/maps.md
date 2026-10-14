# Maps

Amap (`amap`), Google Maps (`gmap`), Baidu Map (`bmap`) and Tencent Map
(`qqmap`) go through their official web-service APIs with your own key
(`atp guide login <platform>`), and take the same commands:

| Command | amap | gmap | bmap | qqmap |
| --- | --- | --- | --- | --- |
| `search`, `read`, `nearby`, `tips` | ✓ | ✓ | ✓ | ✓ |
| `geocode`, `regeo`, `route`, `distance`, `staticmap` | ✓ | ✓ | ✓ | ✓ |
| `download` (place photos) | ✓ | ✓ | | |
| `weather`, `ip` | ✓ | | ✓ | ✓ |
| `traffic` | ✓ | | ✓ | |
| `district` | ✓ | | | ✓ |
| `convert` (by the API) | ✓ | | | |
| `timezone`, `elevation` | | ✓ | | |

    atp amap search 咖啡 city:上海
    atp gmap nearby 2.2945,48.8584 --filter cafe --radius 800
    atp bmap route 西直门 北京南站 --type transit
    atp qqmap distance 中关村 --to 国贸 --to 北京南站 --mode driving

## Points and `--coord`

Points are `LNG,LAT`, longitude first, on every map platform. China's maps
use their own coordinate systems, so the same place has different numbers
on each:

- `wgs84`: GPS, OpenStreetMap; Google outside mainland China.
- `gcj02`: Amap and Tencent; Google inside mainland China.
- `bd09`: Baidu.

`--coord wgs84|gcj02|bd09` names the system of the points you give and of
every point printed. Each platform defaults to its own (`amap` and `qqmap`
gcj02, `bmap` bd09, `gmap` wgs84). ATP converts what you give before the
request and what comes back after it with the public formulas: WGS-84 to
GCJ-02 by the formula itself, back by refining until the two agree, and
GCJ-02 to and from BD-09 by Baidu's rotation (Baidu itself converts its
answers when it can). Printed points say their system:
`{lat: 39.90923, lng: 116.397428, system: gcj02}`.

    atp amap regeo 116.3912,39.9067 --coord wgs84    # a GPS point in Beijing
    atp bmap nearby 116.3912,39.9067 --coord wgs84 --filter food
    atp gmap search 'Peking duck' city:Beijing --coord gcj02

Google's map data in mainland China is GCJ-02 (Hong Kong, Macao and
Taiwan are not): `gmap` sends points there in GCJ-02 and reads Google's
answers there as GCJ-02, converting both from and to the `--coord` system
(WGS-84 unless you say otherwise).

## Results

- Places (`search`, `nearby`, `tips`, `geocode`, `regeo --pois`, `read`)
  are posts with a `place`: `location`, `address`, `category`, `phone`,
  `rating`, `distance_m` (from the point searched around), `country`,
  `province`, `city`, `district`, `adcode`, `hours`, `website`. Their `#N`
  works with `read`.
- `route` gives `mode`, `origin` and `destination` (`name`, `location`)
  and `routes`, each with `distance_m`, `duration_s`, `label`, `tolls` /
  `fare` (prices), `steps` (`instruction`, `mode` for transit legs, `road`,
  `distance_m`, `duration_s`) and `polyline` (`system` and Google's
  encoded polyline format, precision 5).
- `distance` gives one row per origin and destination: `from`, `to`,
  `mode`, `distance_m`, `duration_s`.

In `-f jsonl` / `csv`, a route plan becomes a `section: route` row per route
and a `section: step` row per step (`route`, `step` count from 1).
