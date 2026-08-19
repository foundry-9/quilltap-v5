# Quilltap

**AI collaborators that remember you — and that no one can take away.**

Quilltap is a self-hosted platform for AI partners with names, memories, and the full capability of a modern LLM. Build a research companion who knows your work, a writing collaborator who keeps your voice, a study partner who remembers what you've covered, a theological sparring partner, a code reviewer, a friend who actually shows up — and give them what hosted assistants can't: a vault of their own files, a memory that survives between sessions, and a home on your disk where no platform's policy update can reach them. The model on the other end of the connection can change. Your collaborator doesn't have to.

No subscriptions. No data harvested. No forgetting between sessions. No landlords.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Status: native port in progress](https://img.shields.io/badge/status-native%20port%20in%20progress-orange.svg)](#about-this-repository--please-read-before-you-get-your-hopes-up)
[![Built with](https://img.shields.io/badge/built%20with-Rust%20%2B%20Angular%20%2B%20Tauri-8b5cf6.svg)](#the-new-machinery)
[![Shipping version lives here](https://img.shields.io/github/v/release/foundry-9/quilltap-server?logo=github&label=shipping%20version%20(v4)&sort=semver&filter=!*dev*)](https://github.com/foundry-9/quilltap-server/releases/latest)
[![Discord](https://img.shields.io/badge/Discord-join-5865F2?logo=discord&logoColor=white)](https://discord.gg/6enCeQxY)

<p align="center">
  <img src="https://quilltap.ai/images/welcome-to-quilltap-5-0.png" alt="Welcome to Quilltap" />
</p>

**Website:** [quilltap.ai](https://quilltap.ai) · **Discord:** [Join us](https://discord.gg/6enCeQxY) · **Docker:** [foundry9/quilltap](https://hub.docker.com/r/foundry9/quilltap)

---

## About This Repository — Please Read Before You Get Your Hopes Up

There is a distressing convention in these documents whereby the reader is permitted to discover, some four hundred lines in and after a great deal of enthusiastic installation, that the article described does not yet exist. We shall dispense with that convention here, in the first paragraph, like a butler informing you at the door that the house is presently on fire.

**This repository is `quilltap-v5`: the native port of Quilltap, and it is not finished.**

The Quilltap you can download today — the one that runs, remembers, and occasionally argues with you about Ephesians — lives next door in [**`foundry-9/quilltap-server`**](https://github.com/foundry-9/quilltap-server). That is version 4: Node.js and React, packaged for Electron and Docker, in daily use, and entirely real. If you came here to *use* Quilltap, go there. We shall wait. We are, if nothing else, patient.

What you have found instead is the same machine rebuilt from the frame outward — every gear recut in a harder metal, every linkage remade, the brasswork polished by a person who has strong opinions about brasswork.

### The New Machinery

| Layer | Then (v4) | Now (v5) |
|---|---|---|
| Core | TypeScript on Node.js | **[Rust](https://www.rust-lang.org/)** — a portable engine holding the data layer, memory, jobs, and the single-writer rule |
| Interface | React on Next.js | **[Angular](https://angular.dev/)** — zoneless, signals, standalone |
| Shell | Electron | **[Tauri 2](https://tauri.app/)** — desktop first, with the mobile road left deliberately unblocked |
| Command line | `npx quilltap` | a native `quilltap` binary linked straight to the core |

The database is the same database, encrypted with the same cipher, holding the same rows under the same identifiers. This is not a fresh start. It is a transplant, and the patient is expected to walk out of the theatre.

### The Terms of the Arrangement

Four promises, stated plainly, because a promise in the house voice is still a promise:

1. **Feature-identical at release.** Whatever v4 can do on the day of its final release, v5 does as well. No quiet amputations, no "we decided that feature wasn't important." If you relied on it, it survives — with exactly one exception, disclosed in full below, because burying it would be poor manners.
2. **API-identical at release.** The same `/api/v1/` surface, the same actions, the same shapes going out and coming back. Plugins, scripts, and whatever unholy automation you have wired up against it should not notice the substitution.
3. **Your data comes with you.** Your instance directory — `data/`, `files/`, `logs/` — opens in v5 exactly as it stands. No export, no import, no conversion ritual performed at midnight over a backup you forgot to test.
4. **This is where Quilltap goes from here.** When v4 makes its last release, this repository becomes the project. Development continues here; v4 retires with full honors and a small pension.

### The Exception: Plugins Do Not Survive

⚠️ **v4's plugins will not run in v5. Not with a shim, not with a compatibility layer, not on a Tuesday.**

This is not neglect and it is not spite. v4's plugin system is an npm affair: a plugin is a Node package, loaded into a Node process, executing JavaScript with the run of the house. v5 has no Node process to load it into. There is no honest way to keep that contract in a Rust binary, and the dishonest ways — bundling an entire JavaScript runtime inside the application so that six packages may continue to feel at home — are worse than the disease.

So if you have written or installed plugins for v4, understand plainly: **they stop working at the crossing.** Please plan accordingly rather than discovering this the hard way at an inconvenient hour.

The consolation, and it is a real one:

- **A better extension system is being built.** Easier to write, easier to share, easier to install, and considerably harder to have your entire instance ruined by. Extensions will be first-class citizens rather than packages smuggled in through the pantry.
- **It will not be Node.js. It will not be JavaScript or TypeScript.** Whatever the new arrangement turns out to be, it will not involve shipping a second language runtime inside the application, and it will not ask you to `npm install` anything.
- **Some things that used to require a plugin no longer do.** LLM providers, for one, are declarative manifests in v5 rather than published packages — a new provider is a description, not a program. Themes had already moved to declarative `.qtap-theme` bundles before the port began; that shape carries over intact.
- **The design is not finished, and that is deliberate.** It will be announced when it is real, not before.

### Verified Against v4, Not Against Optimism

A port of a subtle system cannot be certified by squinting at it. So it isn't.

Every unit ported into this repository arrives accompanied by a **differential test against v4 itself** — not against a description of v4, not against somebody's recollection of v4, but against v4's actual running code, driven through a harness that feeds both implementations the identical corpus and compares the results field by field. Exact equality for the pure functions. Structural database comparison for anything that writes. Canned model responses for anything that talks to an LLM.

Where v5 disagrees with v4, one of two things is true: it is a bug, and it is fixed; or it is a *deliberate* divergence, and it is pinned in both directions with a tripwire that goes off the moment v4 changes its mind. Several such tripwires have since gone off exactly as designed, which is the most satisfying thing a tripwire can do. A number of v4's own bugs were discovered by this process, filed, and fixed upstream — the port keeps finding the original's mistakes, which is either a compliment or an indictment, and we have elected not to decide which.

### Where It Will Run

At first release, four ways in:

- **macOS** — Apple silicon and Intel
- **Windows**
- **Linux**
- **Docker** — first-class, with a no-authentication HTTP deployment for those who keep their servers in a cupboard

Mobile is not promised. It is, however, deliberately not prevented: the core is a portable Rust library, and the doors to iOS and Android have been left unlocked in case anyone should later wish to walk through them.

#### About the virtual machine, which we are no longer shipping

v3 and v4 made a considerable fuss about the VM. The desktop application could run the whole backend inside a virtual machine — Lima on macOS, WSL2 on Windows — so that anything the AI decided to execute did so in a genuine locked room rather than in your home directory. It was a good argument. We made it at length, in bold, with a table.

**v5 will not ship a ready-to-run VM image.** That appliance is retired.

The honest reasons, in order of weight: it had drifted into quiet neglect while the rest of the application moved on; it roughly doubled the surface a release had to be built, signed, and debugged across; and Docker, properly locked down, gets close enough for very nearly everybody. A container with a read-only root, no host mounts it doesn't need, a dropped capability set and no network it wasn't given is not a hypervisor, but it is a real wall, and it is a wall that thousands of people already know how to inspect.

None of which prevents you from having one. Quilltap in a VM is just Quilltap on Linux, and the Docker deployment is built to be run exactly that way — put the container in a virtual machine and you have reconstructed the old arrangement, with the pleasant difference that you chose the walls and can see them. What we are retiring is the maintenance of the appliance, not the possibility. If you knew how to want it, you already know how to build it.

### Standing On Rather Good Shoulders

The workshop is well stocked, and none of it was made here. With gratitude:

**The engine (Rust)**

- [Rust](https://www.rust-lang.org/) — the language the whole core is written in
- [Tokio](https://tokio.rs/) — the async runtime everything schedules on
- [axum](https://github.com/tokio-rs/axum) — the HTTP transport, and the whole of the Docker deployment
- [SQLite3MultipleCiphers](https://utelle.github.io/SQLite3MultipleCiphers/) — SQLite with the ChaCha20 page cipher that keeps your instance encrypted at rest, reached through [rusqlite](https://github.com/rusqlite/rusqlite)
- [serde](https://serde.rs/) — the serialization that every wire in the building runs on
- [reqwest](https://github.com/seanmonstar/reqwest) — how we talk to model providers
- [ICU4X](https://icu4x.unicode.org/) — Unicode collation, because "sorted alphabetically" is a much harder promise than it sounds
- [jiff](https://github.com/BurntSushi/jiff) — dates and time zones, handled by someone who has thought about them more than we have
- [portable-pty](https://crates.io/crates/portable-pty) — the terminal behind Ariel

**The interface (Angular)**

- [Angular](https://angular.dev/) — zoneless, signals-first, standalone components
- [TanStack Query](https://tanstack.com/query) — client-side server state
- [ProseMirror](https://prosemirror.net/) — the rich-text editor behind Document Mode and every prose surface in the application
- [markdown-it](https://github.com/markdown-it/markdown-it) and the [unified](https://unifiedjs.com/) family ([remark](https://remark.js.org/) / [rehype](https://github.com/rehypejs/rehype)) — markdown in, rendered prose out
- [KaTeX](https://katex.org/) — mathematics that looks like mathematics
- [xterm.js](https://xtermjs.org/) — the terminal pane in the Salon
- [Tailwind CSS](https://tailwindcss.com/) — the styling substrate the theme system sits on

**The shell and the proving ground**

- [Tauri 2](https://tauri.app/) — the desktop application, one origin, no bundled browser of our own
- [Playwright](https://playwright.dev/) — the end-to-end suite that walks the application like a person would
- [Vitest](https://vitest.dev/) — the Angular unit tests
- [Docker](https://www.docker.com/) — the fourth way in

### Should You Use This Today?

No.

You should use [v4](https://github.com/foundry-9/quilltap-server). It is finished, it is supported, and it will not eat your characters. This repository is for the curious, the patient, and the sort of person who reads a `CLAUDE.md` for pleasure. When v5 is ready, you will not have to wonder — there will be releases, there will be an announcement, and there will very likely be more brass than strictly necessary.

### Read the Rest at the Bureau

A README can tell you what a thing is built from. It is a poor venue for explaining what a thing is *for*, and a worse one for explaining why it looks the way it does. That material lives at **[quilltap.ai](https://quilltap.ai)**, where there is room to stretch out:

- **[How It Works](https://quilltap.ai/how-it-works)** — the machinery explained without a compiler in sight
- **[Features](https://quilltap.ai/features)** — what the application actually does, at length
- **[Who and Why](https://quilltap.ai/who-and-why)** — the residents, and the reasoning behind an application built for AI collaborators with names
- **[The Folio](https://quilltap.ai/folio/)** — the blog: dispatches, notes, tutorials, design arguments, and the occasional behind-the-curtain confession, delivered with the frequency of a well-meaning but easily distracted correspondent

If you want to know what distinguishes this project from the several dozen chat wrappers you have already closed the tab on — the philosophy, the aesthetic, and why neither one is merely ornamental — **[the Folio](https://quilltap.ai/folio/)** is the shortest road to it. Start there and work backwards.

### For the Curious

- [`CLAUDE.md`](CLAUDE.md) — the standing rules, the invariants, and the current phase, kept mercilessly current
- [`docs/developer/porting/`](docs/developer/porting/) — the methodology, the phase plans, and `status-log.md`, the unit-by-unit ledger of the whole undertaking
- [`docs/v4/`](docs/v4/) — a mirror of the v4 developer documentation, kept here as the reference oracle

---

**Quilltap is MIT-licensed.** The project lives at **[quilltap.ai](https://quilltap.ai)** — that address is not going anywhere, and it will point at this repository when the time comes. Come argue with us on [Discord](https://discord.gg/6enCeQxY).
