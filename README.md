# Velum

**Velum** (Latin: *veil*) is a small desktop app that turns a folder of files
— or something you already have running locally — into a Tor onion service
in a few clicks. Tor runs on *your* machine, your files stay on *your*
machine, and the address and private key it hands you are yours alone.

No account, no hosting service, nothing sent anywhere except what Tor itself
needs to publish your service on the network.

## What it does

- Bundles Tor (no separate install) and manages its lifecycle for you.
- **Static folder mode**: point it at a folder with an `index.html` and
  Velum serves it locally and publishes it as an onion service.
- **Existing site mode**: already have something running on
  `localhost:3000`? Velum just points a fresh onion address at it.
- Shows your `.onion` address with a QR code, a one-click copy button, and a
  private-key export so you can move the identity elsewhere or back it up.
- Remembers your sites and brings back the ones you had published the next
  time you open the app.

## What it deliberately doesn't do

- It doesn't host anything on servers we run - there is no "we" in the
  runtime path, just Tor and your machine.
- It doesn't phone home, collect analytics, or require sign-in.
- It can't guarantee content moderation, uptime, or legal compliance for
  whatever you publish - that responsibility is yours, same as running a
  web server ever was.

## Project layout

```
velum/
├─ src/                  Frontend - plain HTML/CSS/JS, no build step
├─ src-tauri/            Rust backend (Tauri v2)
│  ├─ src/
│  │  ├─ tor.rs          Spawns the bundled Tor process, speaks its control protocol
│  │  ├─ site_server.rs  Tiny local static file server for "static folder" sites
│  │  ├─ store.rs        Local JSON persistence for site records + keys
│  │  ├─ commands.rs     Tauri commands the frontend calls
│  │  └─ lib.rs          Wires it all together
│  ├─ binaries/          Bundled Tor binary (fetched by scripts/, gitignored)
│  └─ icons/             App icons (placeholder - swap before shipping)
├─ scripts/
│  ├─ fetch-tor.ps1      Downloads the official Tor Expert Bundle (Windows)
│  ├─ fetch-tor.sh       Same, for macOS/Linux
│  └─ gen-icon.mjs       Regenerates the placeholder icon
└─ website/              Marketing/docs landing page + download links
```

## Building it yourself

**Prerequisites**
- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) (used only for the Tauri CLI and icon tooling - the frontend itself has no build step)
- Platform build tools Tauri needs: on Windows, the MSVC "Desktop development with C++" workload + WebView2 (usually already present on Windows 10/11); see the [Tauri prerequisites guide](https://v2.tauri.app/start/prerequisites/) for macOS/Linux.

**Fetch the Tor binary** (not committed to git - grab it fresh):

```powershell
# Windows
./scripts/fetch-tor.ps1
```

```bash
# macOS / Linux
./scripts/fetch-tor.sh
```

This downloads the official [Tor Expert Bundle](https://www.torproject.org/download/tor/) from `dist.torproject.org` and drops the binary into `src-tauri/binaries/` with the name Tauri's sidecar system expects. It's gitignored on purpose: fetch it fresh rather than trusting a copy sitting in version control, and re-run the script whenever you want to bump the bundled Tor version.

**Run it in dev mode:**

```bash
npm install --prefix . --no-save @tauri-apps/cli   # first time only, or `cargo install tauri-cli`
npx tauri dev
```

**Build a distributable bundle:**

```bash
npx tauri build
```

> **Windows packaging note:** the Tor Expert Bundle ships `tor.exe` alongside
> a handful of DLLs (`libssl`, `libcrypto`, `libevent`, `zlib`, …).
> `fetch-tor.ps1` places those next to the sidecar binary so `tauri dev`
> finds them, but double-check they land next to `tor.exe` in the final
> installed bundle too (via `bundle.resources` in `tauri.conf.json`) before
> shipping a Windows build to anyone else.

## How it works, technically

1. On launch, Velum spawns the bundled `tor` binary with a fresh data
   directory, cookie authentication enabled, and its SOCKS port disabled
   (Velum only *hosts*, it doesn't need Tor as a browsing proxy).
2. When you publish a site, Velum opens an authenticated connection to
   Tor's control port and sends `ADD_ONION`, which returns a `.onion`
   address and (the first time) a private key. That key is what makes the
   address reproducible - saving it means you can recreate the exact same
   address later, even on a different machine.
3. For "static folder" sites, Velum runs a minimal local HTTP server
   (bound to `127.0.0.1` only) and tells Tor to route the onion service's
   port 80 to it. For "existing site" mode, Tor is pointed straight at
   whatever local port you specify.
4. Unpublishing sends `DEL_ONION`; the address and key are kept locally so
   you can republish later with the same address.
5. Closing Velum stops Tor, which takes every site offline until you reopen
   it - there's no background daemon left running.

See [`src-tauri/src/tor.rs`](src-tauri/src/tor.rs) for the control-protocol
client if you want the exact wire-level detail.

## Security notes

- **The private key *is* the address.** Anyone who obtains a site's private
  key can stand up an identical `.onion` address elsewhere. Treat exported
  keys like passwords.
- Keys are stored in a local JSON file (in your OS's app data directory),
  not synced anywhere. Back it up if the address matters to you.
- Velum doesn't sanitize or scan what you publish - the usual rules for
  running any web server apply: keep dependencies patched, don't serve
  secrets by accident, and be deliberate about what you put in a published
  folder.

## License

MIT - see [LICENSE](LICENSE).
