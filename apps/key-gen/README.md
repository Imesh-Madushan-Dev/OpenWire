# Maizy VPN key generator

Issues license keys bound to one computer, one user name and one expiry date.
Plain Node.js, no dependencies. Requires Node 20 or newer.

## One-time setup

```bash
cd apps/key-gen
npm run setup
```

This creates the signing key pair and writes the **public** key into the app
(`apps/desktop-app/src-tauri/license-public.key`). Rebuild the app afterwards.

> **Back up `keys/private.pem` and never commit it.** It is the only thing that
> can issue a key the app accepts. Lose it and no new key will work with the app
> builds you have already shipped; leak it and anyone can issue keys.
> `keys/` is gitignored.

Running `setup` again refuses to overwrite an existing key. `--force` replaces it,
which invalidates every key already issued.

## Issuing a key

1. The user opens the app and copies the **Machine ID** from the activation screen.
2. They send it to you.
3. Run:

   ```bash
   npm start
   ```

   and answer the prompts: Machine ID, user name, and how many days it is valid.
   Or in one line:

   ```bash
   npm start -- --machine A1B2-C3D4-E5F6-0718-293A --name "Jane Doe" --days 30
   ```

4. Send the printed key back. The user pastes it into the app.

Every issued key is also appended to `keys/issued.jsonl`, so you can look up or
re-send a key later.

**Renewing:** issue a new key for the same Machine ID. Pasting it replaces the old one.

## Checking a key a user sent back

```bash
npm run verify -- MZY1.xxxx.yyyy --machine A1B2-C3D4-E5F6-0718-293A
```

Shows the name, machine, expiry and whether the signature is genuine.

## How it holds up

- **Keys cannot be forged or edited.** Each key is an Ed25519 signature. Changing
  any byte - name, machine, expiry - breaks it, and producing a new one needs
  `private.pem`, which never leaves this folder.
- **Keys cannot be shared.** A key only verifies on the machine its ID was taken from.
- **Nothing secret ships in the app.** It contains only the public key, so taking
  the app apart reveals nothing that can mint keys.
- **Winding the clock back is caught** beyond a 48-hour tolerance.

What it cannot do - what no offline license can do - is stop someone patching the
check out of the app binary itself. The check sits in the native Rust core rather
than the UI, which raises that bar considerably, but it is still possible. The
dependable way to cut off a user is on your server: give each user their own
VLESS UUID and remove it when their license ends. Without the server, the app
has nothing to connect to.

A Machine ID changes if Windows is reinstalled. Issue that user a new key.
