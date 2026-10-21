# IP to country database

`dbip-country-lite.mmdb` is DB-IP's free **IP to Country Lite** database, in
MaxMind's MMDB format. `src/core/geo.rs` compiles it into the binary and reads
it in place, so a lookup never touches the disk or the network - the hosts a
user reaches never leave the device to be located.

- Source: https://db-ip.com/db/download/ip-to-country-lite
- Licence: [Creative Commons Attribution 4.0](https://creativecommons.org/licenses/by/4.0/).
  The attribution is shown in Settings > About ("IP geolocation by DB-IP").
- Edition: 2026-10

## Updating

DB-IP publishes a new edition at the start of each month:

```sh
curl -LO https://download.db-ip.com/free/dbip-country-lite-YYYY-MM.mmdb.gz
gzip -d dbip-country-lite-YYYY-MM.mmdb.gz
mv dbip-country-lite-YYYY-MM.mmdb dbip-country-lite.mmdb
```

Then update the edition above. The network monitor service runs its own copy
of the binary, so it picks up a new edition when it is next reinstalled.
