#!/usr/bin/env python3
"""P4.D226 (v4 `4d370a90f`, #75): derive `restore-archive-concierge-legacy.zip`
from `restore-archive.zip`.

Every other committed archive's chats carry NEITHER legacy Concierge key and no
`conciergeMode`, so the restore's `withConciergeModeFromLegacy` wrap can only
ever take its Moderated row there. This archive gives the one instance's chats
the four rows v4's derive table distinguishes, entry for entry the same archive
otherwise (entry order, names and every other byte kept):

  c…0001  conciergeOverride 'UNCENSORED'                → unmoderated/operator/migration
  c…0002  conciergeOverride 'OFF' + isDangerousChat     → locked/operator/migration (the override wins)
  c…0003  (clone, no messages) isDangerousChat only     → unmoderated/concierge/classifier
  c…0004  (clone, no messages) a 4.10 row: conciergeMode
          'locked' by operator/manual OVER a stale
          'UNCENSORED' override                         → left alone (locked)

The manifest's `counts.chats` moves 2 → 4 (the two clones carry no messages,
so `counts.messages` stays 2).

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-concierge-legacy.py
"""
import copy
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
DST = ARCHIVES / "restore-archive-concierge-legacy.zip"

C1 = "c1000000-0000-4000-8000-000000000001"
C2 = "c1000000-0000-4000-8000-000000000002"
C3 = "c1000000-0000-4000-8000-000000000003"
C4 = "c1000000-0000-4000-8000-000000000004"


def rewrite_chats(chats):
    by_id = {c["id"]: c for c in chats}
    assert set(by_id) == {C1, C2}, sorted(by_id)
    by_id[C1]["conciergeOverride"] = "UNCENSORED"
    by_id[C2]["conciergeOverride"] = "OFF"
    by_id[C2]["isDangerousChat"] = True
    c3 = copy.deepcopy(by_id[C1])
    c3.pop("conciergeOverride")
    c3.update(id=C3, title="The Classified Room", isDangerousChat=True, messages=[],
              messageCount=0)
    c4 = copy.deepcopy(by_id[C1])
    c4.update(id=C4, title="The Locked Room", conciergeMode="locked",
              conciergeModeSetBy="operator", conciergeModeReason="manual",
              messages=[], messageCount=0)
    return chats + [c3, c4]


def main():
    with zipfile.ZipFile(SRC) as zin, zipfile.ZipFile(DST, "w", zipfile.ZIP_DEFLATED) as zout:
        for info in zin.infolist():
            data = zin.read(info.filename)
            if info.filename.endswith("/data/chats.json"):
                data = (json.dumps(rewrite_chats(json.loads(data)), indent=2) + "\n").encode()
            elif info.filename.endswith("/manifest.json"):
                m = json.loads(data)
                assert m["counts"]["chats"] == 2
                m["counts"]["chats"] = 4
                data = (json.dumps(m, indent=2) + "\n").encode()
            out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            out.external_attr = info.external_attr
            out.compress_type = zipfile.ZIP_DEFLATED
            zout.writestr(out, data)
    print(f"wrote {DST.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
