# DeepSeek Harness Desktop

**English** · [Español](README.es.md) · [Português](README.pt.md)

Desktop application for [DeepSeek Harness](https://www.deepseek.com/harness/):
it starts the local server, shows a loading screen and opens the interface in
its own window — **no browser**.

> DeepSeek ships no Linux client (macOS `.dmg` and Windows `.exe` only).
> This project fills that gap with Tauri + WebKitGTK.

![Loading screen](captura-splash.png)

## Install

### Option A — `.deb` package

Download the `.deb` from [Releases](../../releases) and install it:

```bash
sudo apt install ./dsh-desktop_0.1.0_amd64.deb
```

**DeepSeek Harness** will appear in your applications menu. To remove it:

```bash
sudo apt remove deep-seek-harness
```

### Option B — Local installer (no root)

```bash
git clone https://github.com/vicman/deepseek-harness-desktop
cd deepseek-harness-desktop
./instalar.sh
```

Everything stays under `~/.local`. It also accepts `--deb` (build the package)
and `--desinstalar` (uninstall).

## Requirements

| Component | Version | Install |
|---|---|---|
| Rust | ≥ 1.77 | `curl https://sh.rustup.rs -sSf \| sh` |
| WebKitGTK | 4.1 | `sudo apt install libwebkit2gtk-4.1-dev` |
| Node.js | **≥ 22** | `nvm install 24` |
| `dsh` | 0.2.0-rc.2 | **automatic** (see below) |

> **Node matters.** DSH uses `node:util.parseEnv`, which does not exist in
> Node 18. The application finds the newest nvm version on its own.

The `.deb` already declares `libwebkit2gtk-4.1-0` and `libgtk-3-0`. You install
Node yourself.

### DSH management

The application **does not bundle DSH**: it resolves it on every launch, so it
never gets stuck on a stale copy.

- **If DSH is missing**, the application installs it on startup
  (`pnpm add -g`, falling back to `npm install -g`). The loading screen shows
  *"Installing DeepSeek Harness…"* meanwhile.
- **If a newer version is published**, the loading screen says so and shows
  the command to update.

**It does not self-update.** Installing software in the background without
asking is your call, not the application's: it only informs, and you run:

```bash
pnpm add -g @deepseek-ai/dsh
```

To compare the installed version against the published one:

```bash
dsh --version
npm view @deepseek-ai/dsh version
```

## Languages

The interface is available in **Spanish, English and Portuguese**. The
language is picked automatically, in this order:

1. The system locale (`LANG`, `LC_MESSAGES`, `LC_ALL`) — e.g. `es_CO.UTF-8`.
2. Your preferred browser languages.
3. **Spanish by default** if none of the three matches.

## How it works

```
dsh-desktop
   │
   ├─ 1. Serves the loading screen at 127.0.0.1:<free port>
   │
   ├─ 2. Starts `dsh web --port 3081` as a child process
   │
   ├─ 3. Waits until the server truly responds (a real HTTP request)
   │
   ├─ 4. Navigates with the token; the webview stores the cookie
   │
   └─ 5. On window close, terminates the child process
```

### Design decisions

**The port is pinned to 3081.** DSH's session cookie has the `host:port`
(`authority`) signed into it. With a random port, cookies from dead sessions
pile up under `127.0.0.1` and the server answers `401`. If 3081 is taken, it
looks for another free port.

**The loading screen is served from `127.0.0.1`, not from `tauri://`.** DSH's
cookie is `SameSite=Strict`: if the splash came from the internal scheme, the
following navigation would be cross-site and the browser **would not send the
cookie** (blank page, `401`).

**WebKitGTK's DMABUF renderer is disabled.** On NVIDIA GPUs, WebKitGTK aborts
with `Could not create GBM EGL display: EGL_NOT_INITIALIZED`. The application
sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` on its own.

## Customizing

`ui/splash.html` is read **at runtime**, so you can change colors, text or the
logo and just reopen the application — no recompiling.

## Build

```bash
cd src-tauri
cargo build --release            # binary
cargo tauri build --bundles deb  # .deb package
```

## Known issues

**Blank window or `401` error.** Usually a stale cookie:

```bash
rm -f ~/.local/share/com.vicmandev.dsh.desktop/cookies
```

**Brief white flash** between the loading screen and the interface (~1 s).
That is WebKit painting its canvas while the document loads. Not yet fixed.

**Stuck on the splash.** Check `dsh --version` and that Node is ≥ 22.

## Layout

```
.
├── instalar.sh              installer / uninstaller
├── ui/
│   ├── splash.html          loading screen (editable without recompiling)
│   └── logo-splash.png      logo, transparent background
├── debian/                  package copyright and changelog
└── src-tauri/
    ├── src/main.rs          startup, splash and navigation
    ├── tauri.conf.json
    └── icons/
```

## License

MIT. The DeepSeek Harness logo is a DeepSeek trademark, used solely to
identify the application.

---

Author: **Victor Manuel Agudelo** &lt;vicmandev@gmail.com&gt;
