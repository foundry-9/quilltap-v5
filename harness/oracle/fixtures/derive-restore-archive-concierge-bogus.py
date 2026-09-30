#!/usr/bin/env python3
"""P4.130 (P4.124 item 14's family-level plant): derive
`restore-archive-concierge-bogus.zip` from `restore-archive.zip`.

The archive's two chats are kept byte for byte; ONE clone of c…0001 is added —
message-less and dependent-less (no message, memory, file or link names it),
so skipping it strands nothing — carrying `conciergeMode: 'bogus'`:

  c…0005  (clone, no messages) conciergeMode 'bogus'   → v4's `withConciergeModeFromLegacy`
          leaves a non-null mode alone, `repos.chats.create` → `validate`
          throws the ZodError, and the restore's per-chat catch skips the chat
          with the warning `Failed to restore chat "The Bogus Room": <ZodError
          message>` — the bytes `summary.warnings` compares verbatim.

The manifest's `counts.chats` moves 2 → 3 (the clone carries no messages, so
`counts.messages` stays 2). Entry order, names and every other byte kept (the
`derive-restore-archive-concierge-legacy.py` precedent).

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-concierge-bogus.py
"""
import copy
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
DST = ARCHIVES / "restore-archive-concierge-bogus.zip"

C1 = "c1000000-0000-4000-8000-000000000001"
C2 = "c1000000-0000-4000-8000-000000000002"
C5 = "c1000000-0000-4000-8000-000000000005"


def rewrite_chats(chats):
    by_id = {c["id"]: c for c in chats}
    assert set(by_id) == {C1, C2}, sorted(by_id)
    c5 = copy.deepcopy(by_id[C1])
    c5.update(id=C5, title="The Bogus Room", conciergeMode="bogus", messages=[],
              messageCount=0)
    return chats + [c5]


def main():
    with zipfile.ZipFile(SRC) as zin, zipfile.ZipFile(DST, "w", zipfile.ZIP_DEFLATED) as zout:
        for info in zin.infolist():
            data = zin.read(info.filename)
            if info.filename.endswith("/data/chats.json"):
                data = (json.dumps(rewrite_chats(json.loads(data)), indent=2) + "\n").encode()
            elif info.filename.endswith("/manifest.json"):
                m = json.loads(data)
                assert m["counts"]["chats"] == 2
                m["counts"]["chats"] = 3
                data = (json.dumps(m, indent=2) + "\n").encode()
            out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            out.external_attr = info.external_attr
            out.compress_type = zipfile.ZIP_DEFLATED
            zout.writestr(out, data)
    print(f"wrote {DST.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
