#!/usr/bin/env python3
"""P4.D355: derive `restore-archive-log-entries.zip` from `restore-archive.zip`
(v4 `cc5f2347c` — the Log rides a logical backup — at the `236c04e35` pin).

No committed archive carries `data/log-entries.json`, so no restore can be
seen running step 22n-quater. Rebuilding the archive family would move EVERY
archive (the P4.D264 DERIVE rule), so this is a DERIVATION of the base —
every other entry and byte is kept.

`data/log-entries.json` (NEW) — four rows in `LogEntrySchema` key order
(`id, message, chatId, characterId, createdAt, updatedAt`):
  d5550000…01  chat c…01, written by LORIAN (a character the archive
               carries — the new-account remap re-keys it with his row)
  d5550000…02  chat c…02, `characterId: null` (the operator ran a tool with
               no character seat — `remapFields` passes the null through)
  d5550000…03  chat c…01, written by a character the archive does NOT carry
               (a1…99 — the remapper's rule for an unknown id, measured);
               its `createdAt` PRECEDES d5550000…01's (an ascending re-read
               reorders the two)
  d5550000…04  a REFUSED row: `message: ""` — `LogEntrySchema`'s `min(1)`;
               every id a valid UUID, so the ONE issue is the message and
               the warning `Failed to restore Log entry: <ZodError text>` is
               compared byte for byte

`manifest.counts.logEntries: 4` is APPENDED LAST (v4 `backup-service.ts:509`)
through the shared helper's optional `counts` patch.

`restore-archive.zip` is md5-checked unchanged before and after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-log-entries.py
"""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from restore_archive_derive import derive  # noqa: E402

LORIAN = "a1000000-0000-4000-8000-000000000001"
STRANGER = "a1000000-0000-4000-8000-000000000099"
C1 = "c1000000-0000-4000-8000-000000000001"
C2 = "c1000000-0000-4000-8000-000000000002"


def row(n, message, chat, character, at):
    return {
        "id": f"d5550000-0000-4000-8000-00000000000{n}",
        "message": message,
        "chatId": chat,
        "characterId": character,
        "createdAt": at,
        "updatedAt": at,
    }


LOG_ENTRIES = [
    row(1, "Lorian noted the tide tables.", C1, LORIAN, "2026-07-20T10:00:00.000Z"),
    row(2, "The operator logged a line with no seat.", C2, None, "2026-07-20T11:00:00.000Z"),
    row(3, "A stranger's line — not carried.", C1, STRANGER, "2026-07-20T09:00:00.000Z"),
    row(4, "", C2, None, "2026-07-20T12:00:00.000Z"),
]


def plan(_zin, _root):
    return {"log-entries": LOG_ENTRIES}


derive("restore-archive-log-entries.zip", plan, counts={"logEntries": len(LOG_ENTRIES)})
