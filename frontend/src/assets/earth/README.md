# Bundled Earth geometry

These SVGs are generated from Natural Earth Admin-0 country polygons and are displayed using the
Equal Earth projection. Natural Earth data is public domain.

- Source repository: <https://github.com/nvkelso/natural-earth-vector>
- Dataset version at generation: 5.1.1
- Low detail: `geojson/ne_110m_admin_0_countries.geojson`
- High detail: `geojson/ne_10m_admin_0_countries.geojson`

Regenerate:

```sh
cargo run -p syllepsis-core --example generate_earth_basemap -- \
  /path/to/ne_110m_admin_0_countries.geojson \
  frontend/src/assets/earth/countries-equal-earth-low.svg 1:110m
cargo run -p syllepsis-core --example generate_earth_basemap -- \
  /path/to/ne_10m_admin_0_countries.geojson \
  frontend/src/assets/earth/countries-equal-earth-high.svg 1:10m
```

The application ships only these projected, quantized SVGs. It does not parse GIS formats or
download map data at runtime.
