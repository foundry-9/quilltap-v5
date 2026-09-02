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
docker run --rm -p 127.0.0.1:3000:3000 -v quilltap-data:/app/quilltap \
  --add-host=host.docker.internal:host-gateway quilltap
```

Then open <http://127.0.0.1:3000/>. First run lands on the setup screen; see
[First run](#first-run-setup-and-unlock).

The `--add-host` flag is what lets a model server on **your machine** —
Ollama, LM Studio, llama-server — be reachable from inside the container. It
is harmless on macOS and Windows, where Docker Desktop resolves
`host.docker.internal` on its own, and **required on Linux**, where Docker
provides no such name. See [Reaching a model server on the
host](#reaching-a-model-server-on-the-host).

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
  --add-host=host.docker.internal:host-gateway \
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

#### Filesystem document stores have to be bound in, and only at creation

A container sees the host filesystem only where you handed it a bind mount.
Database-backed document stores are fine — they live inside the databases,
which live in the data volume. **Filesystem and Obsidian stores are not**:
their `basePath` points anywhere on the host, and inside the container that
path is simply absent.

The failure is a quiet one, which is why it is worth knowing before you meet
it. The store lists its folders quite happily, because that listing comes
from the cached mount index in the database rather than from disk. Only
operations that touch real bytes notice. Creating a folder in such a store is
the one that will tell you:

```
409  The path '/Users/you/Notes' is not visible from inside the container.
     Filesystem document stores must be passed through as bind mounts, which
     can only be done when the container is created. Re-run the start script
     with `--recreate` to rebuild the container with this store included.
```

Bind each store at **the same path inside the container as outside** — the
`basePath` recorded in the database is one string and must mean the same
thing on both sides:

```bash
docker run --rm -p 127.0.0.1:3000:3000 -v quilltap-data:/app/quilltap \
  --add-host=host.docker.internal:host-gateway \
  -v /Users/you/Notes:/Users/you/Notes \
  -e QUILLTAP_TIMEZONE=America/Chicago quilltap
```

Binds are fixed when the container is created, so a store added later needs
the container recreated with the new bind. Two other things worth knowing:
the container runs as a non-root user, so a store the host user can read may
still come back as `exists but cannot be read`; and Docker will happily
fabricate an empty root-owned directory for a bind source that does not
exist, presenting a hollow store as a healthy one — so check the path before
you bind it.

v4 ships a start script that enumerates an instance's filesystem stores and
plans the binds for you (`npm run start:docker`, `quilltap docs
docker-mounts`). v5 has no equivalent yet: it is packaging, not port surface,
and it banks with the standing `quilltap docs` CLI deferral. Until it lands,
the binds are yours to write.

#### Reaching a model server on the host

Inside a container, `localhost` is the *container's* loopback. A connection
profile pointing at `http://localhost:11434` therefore reaches nothing at all —
your Ollama is on the machine outside. Quilltap handles this for you: in a
container it rewrites the host part of any `localhost` / `127.0.0.1` / `[::1]`
base URL to the host gateway before the request goes out, so you can configure
the address you would use on bare metal and leave it alone.

What it rewrites *to* depends on how it is running:

| Situation | Gateway used | What you must do |
| --- | --- | --- |
| Docker Desktop (macOS, Windows) | `host.docker.internal` | nothing |
| Docker on Linux | `host.docker.internal` | pass `--add-host=host.docker.internal:host-gateway` |
| A VM you built and manage yourself | whatever you set | set `QUILLTAP_HOST_IP` |
| Bare metal (desktop, `cargo run`) | none — URLs are left alone | nothing |

Linux Docker provides no built-in name for the host, which is why the flag is
on every `docker run` line above. Without it the rewrite still happens and the
request then fails to resolve — a confusing failure with a one-flag fix.

The bridge gateway address (`172.17.0.1` and friends) is deliberately **not**
used as a fallback. It is only the bridge interface: a server listening on the
host's own loopback is not reachable through it, so falling back to it would
turn "cannot connect" into "connects to the wrong place".

| Variable | What it does | Default |
| --- | --- | --- |
| `QUILLTAP_HOST_IP` | The host address to rewrite `localhost` URLs to. Setting it is also what *enables* rewriting outside Docker — a hand-rolled VM cannot be detected, so this is how you opt in. | unset |

```bash
# A self-managed VM whose host is 10.0.0.5 on the guest network:
QUILLTAP_HOST_IP=10.0.0.5 quilltap-web
```

Two limits worth knowing. An **empty** `QUILLTAP_HOST_IP` counts as unset, not
as "rewrite to nothing". And a profile that carries **no** base URL of its own
falls back to the provider plugin's built-in default, which is not rewritten —
so inside a container, give a local-model profile an explicit
`http://localhost:11434` rather than leaving the field blank.

### Without Docker

```bash
cd apps/web && npm ci && npm run build
```

> Use `npm run build`, not `ng build`: raw `ng` finishes the bundle and then
> never exits (`@angular/build` 21.x holds its esbuild service child open). The
> npm script wraps it via `tools/ng-run.mjs` and returns the real exit code.


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
