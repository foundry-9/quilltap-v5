#!/usr/bin/env python3
"""Generate harness/oracle/fixtures/extraction-enqueue-tier2.json (work order P4.D296).

The spec the shared `build-memory-processor-fixture.ts` builds from (with
`QT_FIXTURE_SPEC=extraction-enqueue-tier2.json`) and the oracle case
`extraction-enqueue-tier2.test.ts` reads: v4's `enqueueMemoryExtraction`
(the strict five-term dedupe, L-6's absent key, `skipDedupCheck`), the batch,
and `handleQueueMemories` (the three profile arms, the greeting turn, the
autonomous bounds, both empty-chat refusals).
Run: python3 harness/oracle/scripts/gen-extraction-enqueue-spec.py — then rebuild
the fixture pair and regenerate the oracle (the Rust family's header has the recipe).
"""
import json
import os
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'fixtures', 'extraction-enqueue-tier2.json')

U = 'a0000000-0000-4000-8000-0000000000e1'
ADA = 'aa000000-0000-4000-8000-0000000000a1'; BRAM = 'bb000000-0000-4000-8000-0000000000b2'; QUILL = 'cc000000-0000-4000-8000-0000000000c3'
P_CUR = '9c000000-0000-4000-8000-000000000001'; P_CHEAP = '9c000000-0000-4000-8000-000000000002'
P_GONE = '9c000000-0000-4000-8000-0000000000ff'
PA = '5a000000-0000-4000-8000-00000000000a'; PB = '5b000000-0000-4000-8000-00000000000b'; PQ = '5c000000-0000-4000-8000-00000000000c'
def cid(n): return f"9a{n:06d}-0000-4000-8000-0000000000{n:02d}"
def mid(n, i): return f"9e{n}{i:05d}-0000-4000-8000-{i:012d}"
def ts(i): h, mn = divmod(i, 60); return f"2026-07-14T{8+h:02d}:{mn:02d}:00.000Z"
def m(n, i, role, pid, text): return {"id": mid(n, i), "role": role, "content": text, "participantId": pid, "createdAt": ts(i)}

# Q1: a salon with a GREETING turn (an assistant line before any user line), then two user turns.
Q1 = [m(1, 1, "ASSISTANT", PA, "Welcome aboard the Gull."), m(1, 2, "USER", PQ, "Where are we bound?"),
      m(1, 3, "ASSISTANT", PA, "North, past the reef."), m(1, 4, "USER", PQ, "And Bram?"),
      m(1, 5, "ASSISTANT", PB, "Below, counting the barrels.")]
# Q2: no seat carries a profile — the cheap-profile arms.
Q2 = [m(2, 1, "USER", PQ, "Morning."), m(2, 2, "ASSISTANT", PA, "Morning, Quill.")]
# Q4: an autonomous room — three assistant anchors, each bounded by the one before.
Q4 = [m(4, 1, "ASSISTANT", PA, "The tide turns."), m(4, 2, "ASSISTANT", PB, "Then we sail."), m(4, 3, "ASSISTANT", PA, "At dawn.")]
# Q5: an autonomous room with only a user line; Q6: a salon with no messages.
Q5 = [m(5, 1, "USER", PQ, "Is anyone here?")]
seats = [{"id": PA, "characterId": ADA, "controlledBy": "llm", "connectionProfileId": P_CUR},
         {"id": PB, "characterId": BRAM, "controlledBy": "llm", "connectionProfileId": P_CUR},
         {"id": PQ, "characterId": QUILL, "controlledBy": "user"}]
bare = [{"id": PA, "characterId": ADA, "controlledBy": "llm"}, {"id": PQ, "characterId": QUILL, "controlledBy": "user"}]
chats = [
    {"id": cid(1), "participants": seats, "messages": Q1},
    {"id": cid(2), "participants": bare, "messages": Q2},
    {"id": cid(4), "chatType": "autonomous", "participants": seats, "messages": Q4},
    {"id": cid(5), "chatType": "autonomous", "participants": seats, "messages": Q5},
    {"id": cid(6), "participants": seats, "messages": []},
]

def turn(chat, opener, anchor=None, **extra):
    """A payload in v4's writer key order."""
    p = {"chatId": chat, "turnOpenerMessageId": opener, "extractionAnchorMessageId": anchor}
    if "extractAfterMessageId" in extra: p["extractAfterMessageId"] = extra.pop("extractAfterMessageId")
    p["connectionProfileId"] = P_CUR
    if "pass_" in extra: p["pass"] = extra.pop("pass_")
    for k in ("cursor", "ignoreIdle", "backfill"):
        if k in extra: p[k] = extra.pop(k)
    assert not extra, extra
    return p

C1, C3 = cid(1), cid(3)
o1, o2, a1 = mid(1, 2), mid(1, 4), mid(1, 3)
ops = [
    # The strict five-term dedupe: chatId, opener, anchor (`?? null`), pass (`turn` default), cursor (`?? null`).
    {"name": "first_turn_job", "kind": "enqueue", "payload": turn(C1, o1)},
    {"name": "same_turn_dedupes", "kind": "enqueue", "payload": turn(C1, o1)},
    {"name": "other_opener_is_new", "kind": "enqueue", "payload": turn(C1, o2)},
    {"name": "anchor_differs_is_new", "kind": "enqueue", "payload": turn(C1, o1, a1)},
    {"name": "explicit_turn_pass_dedupes_the_default", "kind": "enqueue", "payload": turn(C1, o1, pass_="turn")},
    {"name": "extract_after_is_not_a_term", "kind": "enqueue", "payload": turn(C1, o1, extractAfterMessageId=mid(1, 1))},
    {"name": "backfill_is_not_a_term", "kind": "enqueue", "payload": turn(C1, o1, backfill=True)},
    {"name": "fold_pass_is_new", "kind": "enqueue", "payload": turn(C1, None, mid(1, 5), pass_="other-fold")},
    {"name": "fold_cursor_is_a_term", "kind": "enqueue", "payload": turn(C1, None, None, pass_="other-fold", cursor=mid(1, 3))},
    {"name": "fold_same_cursor_dedupes", "kind": "enqueue", "payload": turn(C1, None, None, pass_="other-fold", cursor=mid(1, 3), ignoreIdle=True)},
    # A pre-refactor pending job carries the legacy `foldOtherCatchup` flag (no current writer emits it): planted
    # as a rewritten row, it is the same pass to `memoryExtractionPassOf`, so a current `other-fold` job dedupes on it.
    {"name": "plant_legacy_fold_job", "kind": "enqueue", "payload": turn(C1, None, mid(1, 4), pass_="other-fold"), "options": {"skipDedupCheck": True},
     "rewritePayload": json.dumps({"chatId": C1, "turnOpenerMessageId": None, "extractionAnchorMessageId": mid(1, 4), "connectionProfileId": P_CUR, "foldOtherCatchup": True}, separators=(",", ":"))},
    {"name": "legacy_flag_is_the_same_pass", "kind": "enqueue", "payload": turn(C1, None, mid(1, 4), pass_="other-fold")},
    {"name": "unknown_pass_compares_raw", "kind": "enqueue", "payload": turn(C1, None, mid(1, 5), pass_="bogus")},
    {"name": "skip_dedup_check_always_new", "kind": "enqueue", "payload": turn(C1, o1), "options": {"skipDedupCheck": True}},
    # L-6: a pending job whose payload LACKS `turnOpenerMessageId` (no v4 writer does this) — v4's `===`
    # never matches it against an incoming `null`; one whose anchor key is absent does match (`?? null`).
    {"name": "plant_absent_opener", "kind": "enqueue", "payload": turn(C3, None, mid(3, 9)), "options": {"skipDedupCheck": True}, "stripKey": "turnOpenerMessageId"},
    {"name": "absent_opener_never_matches_null", "kind": "enqueue", "payload": turn(C3, None, mid(3, 9))},
    {"name": "plant_absent_anchor", "kind": "enqueue", "payload": turn(C3, mid(3, 1)), "options": {"skipDedupCheck": True}, "stripKey": "extractionAnchorMessageId"},
    {"name": "absent_anchor_matches_null", "kind": "enqueue", "payload": turn(C3, mid(3, 1))},
    # The batch: `backfill: true`, `extractAfterMessageId` only when set, the priority option.
    {"name": "batch_bytes", "kind": "batch", "chatId": C3, "entries": [
        {"turnOpenerMessageId": mid(3, 1), "extractionAnchorMessageId": None},
        {"turnOpenerMessageId": None, "extractionAnchorMessageId": mid(3, 4), "extractAfterMessageId": mid(3, 3)}],
     "priority": 2},
    {"name": "batch_empty", "kind": "batch", "chatId": C3, "entries": [], "priority": 0},
    # queue-memories, whole.
    {"name": "qm_seat_profile_with_greeting", "kind": "queueMemories", "chatId": cid(1)},
    {"name": "qm_second_press_stacks", "kind": "queueMemories", "chatId": cid(1)},
    {"name": "qm_default_cheap_profile", "kind": "queueMemories", "chatId": cid(2),
     "plants": {"cheapLLMSettings": {"strategy": "PROVIDER_CHEAPEST", "fallbackToLocal": False, "defaultCheapProfileId": P_CHEAP}}},
    {"name": "qm_user_defined_profile", "kind": "queueMemories", "chatId": cid(2),
     "plants": {"cheapLLMSettings": {"strategy": "USER_DEFINED", "fallbackToLocal": False, "userDefinedProfileId": P_CHEAP}}},
    {"name": "qm_user_defined_needs_the_strategy", "kind": "queueMemories", "chatId": cid(2),
     "plants": {"cheapLLMSettings": {"strategy": "PROVIDER_CHEAPEST", "fallbackToLocal": False, "userDefinedProfileId": P_CHEAP}}},
    {"name": "qm_missing_profile_400", "kind": "queueMemories", "chatId": cid(2),
     "plants": {"cheapLLMSettings": {"strategy": "PROVIDER_CHEAPEST", "fallbackToLocal": False, "defaultCheapProfileId": P_GONE}}},
    {"name": "qm_autonomous_bounds", "kind": "queueMemories", "chatId": cid(4)},
    {"name": "qm_autonomous_empty_400", "kind": "queueMemories", "chatId": cid(5)},
    {"name": "qm_salon_empty_400", "kind": "queueMemories", "chatId": cid(6)},
]
spec = {
    "testPepperBase64": "dGVzdC1wZXBwZXItZm9yLWZpeHR1cmVzLW9ubHkxMjM0NTY3OA==",
    "userId": U, "seedTimestamp": "2026-07-01T00:00:00.000Z", "futureTimestamp": "2026-07-01T00:00:00.000Z",
    "connectionProfiles": [
        {"id": P_CUR, "name": "Current", "provider": "ANTHROPIC", "modelName": "claude-sonnet-4-5", "isCheap": False, "isDangerousCompatible": False},
        {"id": P_CHEAP, "name": "Cheap", "provider": "OPENAI", "modelName": "gpt-4o-mini", "isCheap": True, "isDangerousCompatible": False}],
    "chatSettings": {"cheapLLMSettings": {"strategy": "PROVIDER_CHEAPEST", "fallbackToLocal": False}},
    "characters": [
        {"id": ADA, "name": "Ada", "controlledBy": "llm", "identity": "Ada captains the Gull.", "othersFiles": []},
        {"id": BRAM, "name": "Bram", "controlledBy": "llm", "identity": "Bram is the quartermaster.", "othersFiles": []},
        {"id": QUILL, "name": "Quill", "controlledBy": "user", "identity": "Quill is a scribe.", "othersFiles": []}],
    "seedMemories": [],
    "chats": chats,
    "embeddingDims": 16,
    "completionRules": [],
    "ops": ops,
}
json.dump(spec, open(OUT, 'w'), indent=1, ensure_ascii=False)
print(len(ops))
