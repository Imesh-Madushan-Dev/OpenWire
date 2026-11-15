# Wintun

`wintun.dll` is [Wintun](https://www.wintun.net), the virtual network adapter
driver from the WireGuard project, used by system-wide routing
(`src/core/tun`). It is WireGuard LLC's signed prebuilt binary, unmodified,
and OpenWire only calls the API in its `wintun.h`, as the licence requires.

- Source: https://www.wintun.net/builds/wintun-0.14.1.zip (`bin/amd64/wintun.dll`)
- Version: 0.14.1
- Zip SHA-256: `07c256185d6ee3652e09fa55c0b673e2624b565e02c4b9091c79ca7d2f24ef51`
- DLL SHA-256: `e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce`
- Signature: Authenticode, `CN=WireGuard LLC`
- Licence: the Prebuilt Binaries License in `LICENSE.txt`, which allows
  shipping the DLL alongside software that uses it only through that API.

The installer puts it beside `OpenWire.exe` (`bundle.resources` in
`tauri.conf.json`). Installing the tunnel service copies it into the
service's folder under Program Files, and the service loads it from there by
full path only.

## Updating

```sh
curl -LO https://www.wintun.net/builds/wintun-X.Y.Z.zip
sha256sum wintun-X.Y.Z.zip   # compare with the hash published on wintun.net
unzip wintun-X.Y.Z.zip
cp wintun/bin/amd64/wintun.dll wintun/LICENSE.txt .
```

Check the signature (`Get-AuthenticodeSignature wintun.dll` in PowerShell),
then update the version and hashes above.
