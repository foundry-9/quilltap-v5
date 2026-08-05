# Running Quilltap v5

Three modes ship, and they are co-equal (Phase-4 **D1**): the **desktop app**,
the **web server**, and the **CLI**. All three link the same `quilltap-core`
and open the same instance directory, so nothing here is a lesser build —
they are three doors into one house.

This is a *dev-grade* document for people working in this repo. Nothing in v5
is released, signed, notarized, or published yet (**D21**), so every mode
below is something you build locally.

---

## Which one do I want?

| I want to… | Mode | Start with |
| --- | --- | --- |
| use Quilltap like an app, on this machine | **Desktop** (Tauri) | [Desktop](#desktop-tauri) |
| run it as a server and use a browser — including on another machine, behind a proxy | **Server** (`quilltap-web`, or Docker) | [Server](#server-quilltap-web) |
| inspect or repair an instance's data | **CLI** (`quilltap`) | [CLI](#cli-quilltap) |

---

## Server (`quilltap-web`)

### With Docker

```bash
docker build -t quilltap .
```

```bash
docker run --rm -p 127.0.0.1:3000:3000 -v quilltap-data:/app/quilltap quilltap
```

Then open <http://127.0.0.1:3000/>. First run lands on the setup screen; see
[First run](#first-run-setup-and-unlock).

The image builds the Rust binaries and the Angular SPA in two concurrent
stages and lays them down FHS-style:

| In the image | What |
| --- | --- |
| `/usr/local/bin/quilltap-web` | the server |
| `/usr/local/bin/quilltap` | the CLI, for `docker exec` |
| `/usr/local/share/quilltap/spa` | the Angular dist |
| `/app/quilltap` | the data dir (a `VOLUME`) |

Your instance lives in the named volume. Keep the volume, keep your data —
and note that losing it loses everything, because the databases are
encrypted with a key that lives in there.

The first build is slow: it compiles the SQLite3MultipleCiphers amalgamation
(~12 MB of C). BuildKit cache mounts and the pinned `quilltap-sqlite3mc-sys`
version mean it happens once per cache, not once per build.

#### Set the timezone, or things fire at the wrong hour

A container has no timezone, so it runs on UTC unless told otherwise — and
that is not merely cosmetic. Some of Quilltap consults the wall clock
directly rather than asking how to phrase a date: rooms that wake on a
schedule, the daily token allowance that turns over at local midnight, and
the Commonplace Book's notion of what counts as "today" for same-day recall.
Leave it unset and a room set for 07:00 rings at 02:00.

```bash
docker run --rm -p 127.0.0.1:3000:3000 -v quilltap-data:/app/quilltap \
  -e QUILLTAP_TIMEZONE=America/Chicago quilltap
```

| Variable | What it does | Default |
| --- | --- | --- |
| `QUILLTAP_TIMEZONE` | IANA zone name (`America/New_York`, `Europe/London`, `Asia/Tokyo`) for the whole process — timestamp injection *and* the clock. | unset → UTC in a container |
| `TZ` | The same thing, by its Unix name. | unset → UTC in a container |

Setting **either one is enough**: the server copies whichever is present into
the other at startup, and `QUILLTAP_TIMEZONE` wins if both are set and
disagree. This is v4's rule (its `docker/entrypoint.sh`), applied in
`quilltap-web`'s `main` because the v5 image runs the binary directly and has
no entrypoint script. `-e QUILLTAP_TIMEZONE=UTC` deliberately pins to UTC.

The value must be an **IANA zone name** — `UTC`, or something with a `/` in
it. An abbreviation like `CDT` is refused with a warning on stderr rather
than forwarded, because forwarding it would fall silently back to UTC, which
is the failure this is here to prevent. Zone lookup reads the tzdb on disk;
the image's `debian:bookworm-slim` base ships `tzdata`, so nothing extra is
needed.

### Without Docker

```bash
cd apps/web && npm ci && npm run build
```

```bash
cargo build --release -p quilltap-web
```

The binary looks for its Angular dist in this order:

1. `--spa-dir <path>`,
2. the `QUILLTAP_SPA_DIR` environment variable,
3. `spa/` beside the binary,
4. `../share/quilltap/spa` relative to the binary (the layout above),
5. nothing — in which case it serves two placeholder pages and *says so* on
   startup.

So the simplest bare deployment is a directory holding the binary and its
`spa/`:

```bash
mkdir -p ~/quilltap-server/spa
cp target/release/quilltap-web ~/quilltap-server/
cp -R apps/web/dist/quilltap/browser/. ~/quilltap-server/spa/
~/quilltap-server/quilltap-web
```

The startup banner tells you which of those five it landed on. If it says the
parlour stands unfurnished, you are looking at placeholder pages, not the app.

### Ports, binding, and the fact that there is no login

**There is no authentication** (**D2**), by decision and not by omission — v4's
session layer was already synthetic single-user, so there was nothing to port.
Everything that can reach the port can use the app and read the data.

- The **bare binary** defaults to `127.0.0.1:3000`. Machine-local.
- The **container** runs `--host 0.0.0.0`, because inside a container that is
  just the container's own network. **The port publish is the real boundary:**
  `-p 127.0.0.1:3000:3000` keeps it on your machine; `-p 3000:3000` puts an
  unauthenticated Quilltap on your network, and everyone on it becomes you.

If you want it reachable from elsewhere, put something in front that provides
authentication and TLS (Caddy, Traefik, a tunnel). Do not widen the bind and
hope.

Flags: `--host`, `--port`, `--data-dir <path>`, `--instance <name>`,
`--spa-dir <path>`. Environment: `QUILLTAP_DATA_DIR`, `QUILLTAP_SPA_DIR`,
`QUILLTAP_TIMEZONE` / `TZ` (see
[Set the timezone](#set-the-timezone-or-things-fire-at-the-wrong-hour)).
`--help` lists them.

---

## Desktop (Tauri)

A build recipe only. **Bundling targets beyond the raw app, signing,
notarization, and the updater are all deferred (D21)** — `tauri.conf.json`
carries `bundle.targets: ["app"]`, so there is no `.dmg`, `.msi`, or `.deb`
here and nothing is signed.

```bash
cd apps/web && npm ci && npm run build
```

```bash
cargo tauri build
```

Tauri reads the dist from `apps/web/dist/quilltap/browser` — the same bundle
the server ships — and serves it over the internal `qtap://` protocol, so the
desktop app has no port and no HTTP surface at all.

There is no turnkey `tauri dev` yet: build the SPA first, then build the shell.

---

## CLI (`quilltap`)

```bash
cargo build --release -p quilltap-cli   # the binary is named `quilltap`
```

In the Docker image it is already on `PATH`:

```bash
docker exec -it <container> quilltap db --tables
```

Subcommands: `db`, `themes`, `docs`, `memories`, `instances`, `logs`,
`migrations`, `maintenance`, `file-verify`, `memory-diff`, `recall-replay`,
`completion`. Global flags: `--data-dir`, `--instance`, `--port`,
`--passphrase`, `--version`.

If the instance has a passphrase, pass `--passphrase` or set
`QUILLTAP_DB_PASSPHRASE`.

### The write refusal is the design, not a bug

The single-writer invariant is **per-process**: exactly one process owns an
instance's databases (**D12**). So against a *running* server, CLI **reads**
work and CLI **writes** refuse:

```
Database is currently in use — held by PID 1 on this host.
Stop the running Quilltap instance before opening the database read-write.
(See `quilltap db --lock-status` for details.)
```

That is correct behavior — the alternative is two processes writing one
encrypted database. To write, stop the server first.

(D12 describes the CLI as dual-mode, direct-core or HTTP-client. As of this
writing only `recall-replay` actually takes the HTTP path; every other
subcommand opens the data directory directly, reads freely, and refuses to
write while a server holds the lock.)

---

## Where the data lives

Resolution order: `--data-dir` → `--instance <name>` (the registry) →
`QUILLTAP_DATA_DIR` → the platform default:

| Platform | Default instance directory |
| --- | --- |
| macOS | `~/Library/Application Support/Quilltap` |
| Windows | `%APPDATA%\Quilltap` |
| Linux | `~/.quilltap` |
| in a container | `/app/quilltap` |

Inside it, `data/` holds the three encrypted databases (main, mount-index,
llm-logs) and `quilltap.dbkey`, the wrapped encryption key.

**The databases are encrypted with ChaCha20-Poly1305 and there is no recovery
path.** Lose the data directory and the data is gone; lose the pepper on a
passphrase-less instance and the data is gone. Back up the whole directory.

---

## First run: setup and unlock

A fresh instance has no encryption key, so the server starts **locked**. That
is not an error state and the server serves happily in it:

- `GET /health` → `423` with `{"status":"locked","dbKeyState":"needs-setup"}`.
- Any gated dispatch → `503` with `{"error":"Setup required","setupUrl":"/setup"}`.
- The browser lands on `/setup`.

The setup screen generates the encryption key for you and offers an optional
passphrase:

- **No passphrase** — the instance unlocks itself on every start. Convenient;
  the key sits unprotected in the data directory.
- **With a passphrase** — every start shows the unlock screen. The passphrase
  wraps the key; it is not stored.

Either way, setup then shows you the **pepper** once, and once only. Save it
somewhere safe. It is the master key: set it as `ENCRYPTION_MASTER_PEPPER` and
you can open the instance without the passphrase — which is the recovery path
and also the reason it must never be committed, logged, or dropped in a synced
folder.

`dbKeyState` values you will see: `needs-setup`, `needs-passphrase`,
`needs-vault-storage`, `resolved`.

### The sample content

A fresh instance seeds itself with sample content — the characters **Lorian**
and **Riya**, their wardrobes, and 42 memories — so the app has something in
it on the first screen. This is **on by default**
(`HostConfig::seed_sample_content`). Delete them if you would rather start
empty; it seeds only on a genuinely fresh instance, never over your data.

---

## Health check

```bash
curl -s http://127.0.0.1:3000/health
```

| Status | Meaning |
| --- | --- |
| `200 healthy` | running, unlocked |
| `423 locked` | running, needs setup or a passphrase (see `dbKeyState`) |
| `409 lock-conflict` | another process already owns this instance |
| `503 unhealthy` | boot failed; the body carries the error |

The server always answers `/health`, even when it could not boot the engine —
a conflicted or failed instance is diagnosable rather than silent.
