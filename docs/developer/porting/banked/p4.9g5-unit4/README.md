# Banked WIP — P4.9G5 unit 4 (the restore orchestrator)

> **⚠ NOT BUILT, NOT REACHABLE FROM ANY CRATE ROOT.** These files live under
> `docs/` deliberately: cargo never sees them, `cargo fmt` never touches them,
> and no `mod` declaration anywhere refers to them. They are an archive, not
> code. Nothing in the workspace depends on this directory and deleting it breaks
> no build.

## What this is

P4.9G5 unit 4 — the port of v4's `lib/backup/restore/restore.ts` — was **written
and compiling** but deliberately **not landed**, because the tier-2 differential
it is required to ship could not be made green without a human ruling on two real
v4 bugs.

**✅ THAT RULING HAS BEEN GIVEN (2026-07-25): "I want this work, not just fail the
same way v4 fails" — v5 diverges on both. Unit 4 is UNBLOCKED.** Read
`status-log.md` → "Ruling — the two v4 restore bugs (2026-07-25)" first; note that
finding 2 requires a deliberate change to `get_file_from_extracted_backup`
(`=== 2` → `>= 2`), and that the divergence is reader-side only — **the backup
writer must stay byte-identical to v4's.**

The full evidence, the ruling, and the resume list are in:

- `docs/developer/porting/work-orders/p4.9g5-backup-restore.md` (status header)
- `docs/developer/porting/status-log.md` — "Lane record — P4.9G5-resumed, unit 4
  (restore execute): NOT LANDED, and why" (every non-obvious porting decision is
  written down there, which is the authoritative record)
- `docs/developer/porting/dogfood-findings.md` — the queue entry under
  "Post-5.0 v4-side FIXES"

The lane originally banked these four files in `/tmp/qt-p4.9g5-unit4-wip/` and
said so plainly ("this machine only, not committed… regenerate from this record
if it is gone"). They were copied here **at the round's unification (2026-07-25)**
so that a reboot does not cost ~1,400 lines of finished work. That is the only
change: no file was edited on the way in.

## Files

| File | What it is |
| --- | --- |
| `restore.rs` | the orchestrator, all 35 phases in v4's order |
| `rows.rs` | the JSON-row accessors it reads archive rows through |
| `system_restore_state.rs` | the tier-2 state differential, with the divergence scaffolding the ruling will fill in |
| `unit4-tracked-edits.diff` | the additive repo methods, the seam, and the engine arm — the edits to files that already exist |

## How to resume

Do **not** copy these in blind. Work the resume list in the order's status header
first — in particular, the ruling and the differential's move to a pre/post
**delta** diff, which changes what `system_restore_state.rs` asserts. The lane
record is the source of truth for intent; these files are the draft that record
describes.

Two things landed on main that unit 4 should build on rather than re-derive:

- `services::backup::uuid_remap::remap_backup_data` (P4.9G6, CLOSED) —
  differential-proven, waiting for its only caller.
- `crates/quilltap-harness/tests/p4_9g6_seam_contract.rs` — pins the §2
  signatures at compile time and proves `parse_backup_zip` → `remap_backup_data`
  composes. **Write the `new-account` call site against those pins, and keep the
  file.**

**Delete this directory once unit 4 lands.**
