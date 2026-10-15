#!/usr/bin/env python3
"""Generate harness/oracle/fixtures/memory-regenerate-jobs-tier3.json (work order P4.D296 R-V).

The spec the shared `build-memory-processor-fixture.ts` builds from (with
`QT_FIXTURE_SPEC=memory-regenerate-jobs-tier3.json`) and the oracle case
`memory-regenerate-jobs-tier3.test.ts` reads: v4's `handleMemoryRegenerateChat`
(the wipe, the cursors cleared, one turn job per turn, the autonomous bounds, the
catch-ups per mode, the orphan arm, the profile throw before any wipe) and
`handleMemoryRegenerateAll` (the fan-out: the in-flight snapshot, the dangerous
route, the orphan chat ids).
Run: python3 harness/oracle/scripts/gen-memory-regenerate-spec.py — then rebuild the
fixture pair and regenerate the oracle (the Rust family's header has the recipe).
"""
import json
import os
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'fixtures', 'memory-regenerate-jobs-tier3.json')

U = 'a0000000-0000-4000-8000-0000000000e1'
ADA = 'aa000000-0000-4000-8000-0000000000a1'; BRAM = 'bb000000-0000-4000-8000-0000000000b2'; QUILL = 'cc000000-0000-4000-8000-0000000000c3'
P_STD = '9c000000-0000-4000-8000-000000000001'; P_DNG = '9c000000-0000-4000-8000-000000000002'
P_GONE = '9c000000-0000-4000-8000-0000000000ff'
PA = '5a000000-0000-4000-8000-00000000000a'; PB = '5b000000-0000-4000-8000-00000000000b'; PQ = '5c000000-0000-4000-8000-00000000000c'
def cid(n): return f"7a{n:06d}-0000-4000-8000-0000000000{n:02d}"
def mid(n, i): return f"7e{n}{i:05d}-0000-4000-8000-{i:012d}"
def ts(i): h, mn = divmod(i, 60); return f"2026-07-14T{8+h:02d}:{mn:02d}:00.000Z"
def m(n, i, role, pid, text): return {"id": mid(n, i), "role": role, "content": text, "participantId": pid, "createdAt": ts(i)}
seats = [{"id": PA, "characterId": ADA, "controlledBy": "llm", "connectionProfileId": P_STD},
         {"id": PB, "characterId": BRAM, "controlledBy": "llm", "connectionProfileId": P_STD},
         {"id": PQ, "characterId": QUILL, "controlledBy": "user"}]
R1 = [m(1, 1, "ASSISTANT", PA, "Welcome aboard."), m(1, 2, "USER", PQ, "Where to?"), m(1, 3, "ASSISTANT", PA, "North."),
      m(1, 4, "USER", PQ, "And Bram?"), m(1, 5, "ASSISTANT", PB, "Below decks.")]
R2 = [m(2, 1, "ASSISTANT", PA, "The tide turns."), m(2, 2, "ASSISTANT", PB, "Then we sail."), m(2, 3, "ASSISTANT", PA, "At dawn.")]
R3 = [m(3, 1, "USER", PQ, "Morning."), m(3, 2, "ASSISTANT", PA, "Morning, Quill.")]
R4 = [m(4, 1, "USER", PQ, "Is it safe?"), m(4, 2, "ASSISTANT", PA, "Nothing aboard is safe.")]
chats = [
    {"id": cid(1), "participants": seats, "messages": R1},
    {"id": cid(2), "chatType": "autonomous", "participants": seats, "messages": R2},
    {"id": cid(3), "participants": seats, "messages": R3},
    # An Unmoderated chat: the fan-out routes it through the dangerous profile.
    {"id": cid(4), "participants": seats, "messages": R4, "row": {"conciergeMode": "unmoderated"}},
]
ORPHAN = cid(9)
def mem(n, char, chat, content, **kw):
    d = {"id": f"5eed00{n:02d}-0000-4000-8000-0000000000{n:02d}", "characterId": char, "chatId": chat, "content": content,
         "summary": content.lower()[:30], "vector": None, "createdAt": "seed"}
    d.update(kw); return d
seeds = [
    mem(1, ADA, cid(1), "Ada set the course north."),
    mem(2, ADA, cid(1), "Ada keeps a private log.", source="MANUAL"),
    mem(3, BRAM, cid(1), "Bram counted the barrels."),
    mem(4, ADA, cid(2), "Ada watched the tide."),
    mem(5, BRAM, ORPHAN, "Bram remembers a harbour that no longer exists."),
    mem(6, ADA, cid(3), "Ada greeted Quill."),
    # A second orphan the per-chat ops never wipe: the fan-out's DISTINCT read finds it.
    mem(7, ADA, cid(8), "Ada recalls a chat long deleted."),
]
rc = lambda chat, prof=P_STD: {"chatId": chat, "connectionProfileId": prof}
ops = [
    {"name": "regen_chat_wipes_and_rebuilds", "kind": "chat", "payload": rc(cid(1)),
     "plants": {"cursor": {"chatId": cid(1), "turn": mid(1, 3), "other-fold": mid(1, 5), "episode-fold": mid(1, 5)}}},
    {"name": "regen_autonomous_bounds", "kind": "chat", "payload": rc(cid(2))},
    {"name": "regen_turn_mode_only_episode", "kind": "chat", "payload": rc(cid(3)),
     "plants": {"mode": {"otherPass": "turn", "perTurnOtherFloor": 0.75, "foldCandidatesPerSubject": 3}}},
    {"name": "regen_profile_gone_throws_before_wiping", "kind": "chat", "payload": rc(cid(4), P_GONE)},
    {"name": "regen_orphan_chat", "kind": "chat", "payload": rc(ORPHAN)},
    # The fan-out: cid(3) already has a wipe in flight (planted), cid(4) is Unmoderated, the orphan was wiped above
    # so only cid(8)'s orphan remains; the second fan-out finds every wipe in flight.
    {"name": "fan_out", "kind": "all", "payload": {"standardProfileId": P_STD, "dangerousProfileId": P_DNG},
     "plants": {"inFlightWipe": cid(3)}},
    {"name": "fan_out_again_skips_every_in_flight_wipe", "kind": "all", "payload": {"standardProfileId": P_STD, "dangerousProfileId": P_DNG}},
]
spec = {
    "testPepperBase64": "dGVzdC1wZXBwZXItZm9yLWZpeHR1cmVzLW9ubHkxMjM0NTY3OA==",
    "userId": U, "seedTimestamp": "2026-07-01T00:00:00.000Z", "futureTimestamp": "2026-07-01T00:00:00.000Z",
    "connectionProfiles": [
        {"id": P_STD, "name": "Standard", "provider": "ANTHROPIC", "modelName": "claude-sonnet-4-5", "isCheap": False, "isDangerousCompatible": False},
        {"id": P_DNG, "name": "Dangerous", "provider": "OPENAI", "modelName": "gpt-4o-mini", "isCheap": True, "isDangerousCompatible": True}],
    "chatSettings": {"cheapLLMSettings": {"strategy": "PROVIDER_CHEAPEST", "fallbackToLocal": False}},
    "characters": [
        {"id": ADA, "name": "Ada", "controlledBy": "llm", "identity": "Ada captains the Gull.", "othersFiles": []},
        {"id": BRAM, "name": "Bram", "controlledBy": "llm", "identity": "Bram is the quartermaster.", "othersFiles": []},
        {"id": QUILL, "name": "Quill", "controlledBy": "user", "identity": "Quill is a scribe.", "othersFiles": []}],
    "seedMemories": seeds,
    "chats": chats,
    "embeddingDims": 16,
    "completionRules": [],
    "ops": ops,
}
json.dump(spec, open(OUT, 'w'), indent=1, ensure_ascii=False)
print(len(ops))
