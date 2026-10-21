# MAC vendor table

`oui.tsv` maps the first 24, 28 or 36 bits of a MAC address to the company the
IEEE assigned them to, so the Devices view can say "Apple" or "Synology"
beside an address. `src/core/lan/vendor.rs` compiles it into the binary.

- Source: the IEEE Registration Authority's public listings,
  https://standards-oui.ieee.org/ - MA-L (`oui/oui.csv`), MA-M
  (`oui28/mam.csv`) and MA-S (`oui36/oui36.csv`).
- Built: 2026-10-21

Each line is `PREFIX<TAB>Vendor`; the prefix's length (6, 7 or 9 hex digits)
says which registry it came from, and the longest match wins. `build.mjs`
drops placeholder rows and trims legal suffixes so names read as brands.

## Updating

```sh
curl -O https://standards-oui.ieee.org/oui/oui.csv
curl -O https://standards-oui.ieee.org/oui28/mam.csv
curl -O https://standards-oui.ieee.org/oui36/oui36.csv
node build.mjs oui.csv mam.csv oui36.csv
```

Then update the build date above.
