#!/usr/bin/env python3
"""Generate harness/oracle/fixtures/fold-other-pass-tier3.json (work order P4.D296).

The spec the shared `build-memory-processor-fixture.ts` builds from (with
`QT_FIXTURE_SPEC=fold-other-pass-tier3.json` when it is not the default) and the oracle case reads.
Run: python3 harness/oracle/scripts/gen-fold-other-pass-spec.py — then rebuild the fixture pair and
regenerate the oracle (the Rust family's header has the recipe).
"""
import os
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'fixtures', 'fold-other-pass-tier3.json')
import json
U='a0000000-0000-4000-8000-0000000000e1'
ADA='aa000000-0000-4000-8000-0000000000a1'; BRAM='bb000000-0000-4000-8000-0000000000b2'
P_CUR='9c000000-0000-4000-8000-000000000001'; P_CHEAP='9c000000-0000-4000-8000-000000000002'
PA='5a000000-0000-4000-8000-00000000000a'; PB='5b000000-0000-4000-8000-00000000000b'
def cid(n): return f"c{n}000000-0000-4000-8000-0000000000c{n}"
K={n:cid(n) for n in range(1,9)}
def mid(n,i): return f"e{n}{i:06d}-0000-4000-8000-{i:012d}"
def ts(i):
    h,mn=divmod(i,60); return f"2026-07-14T{4+h:02d}:{mn:02d}:00.000Z"
def msgs(n,count,tag):
    return [{"id":mid(n,i),"role":"ASSISTANT","content":f"{tag}{i:03d} aboard the Gull, stop {i}.","participantId":PA if i%2 else PB,"createdAt":ts(i)} for i in range(1,count+1)]
seats=[{"id":PA,"characterId":ADA,"controlledBy":"llm","connectionProfileId":P_CUR},{"id":PB,"characterId":BRAM,"controlledBy":"llm","connectionProfileId":P_CUR}]
noprof=[{"id":PA,"characterId":ADA,"controlledBy":"llm"},{"id":PB,"characterId":BRAM,"controlledBy":"llm"}]
M={1:msgs(1,300,'K'),2:msgs(2,10,'B'),3:msgs(3,6,'C'),4:msgs(4,6,'H'),5:msgs(5,1,'S'),6:msgs(6,6,'O'),7:msgs(7,6,'N'),8:msgs(8,6,'D')}
ids=lambda n:[m['id'] for m in M[n]]
chats=[
 {"id":K[1],"participants":seats,"messages":M[1],"lastMessageAt":"2026-07-14T11:00:00.000Z","messageCount":300,"summaryAnchorMessageIds":ids(1)[:130]},
 {"id":K[2],"participants":seats,"messages":M[2],"lastMessageAt":"2026-07-14T10:00:00.000Z","messageCount":10},
 {"id":K[3],"participants":seats,"messages":M[3],"lastMessageAt":"2026-07-14T13:30:00.000Z","messageCount":6,"summaryAnchorMessageIds":ids(3)[:4]},
 {"id":K[4],"chatType":"help","participants":seats,"messages":M[4],"lastMessageAt":"2026-07-14T09:00:00.000Z","messageCount":6,"summaryAnchorMessageIds":ids(4)[:4]},
 {"id":K[5],"participants":seats,"messages":M[5],"lastMessageAt":"2026-07-14T09:00:00.000Z","messageCount":1},
 {"id":K[6],"participants":seats,"messages":M[6],"lastMessageAt":"2026-06-01T09:00:00.000Z","messageCount":6,"summaryAnchorMessageIds":ids(6)[:4]},
 {"id":K[7],"participants":noprof,"messages":M[7],"lastMessageAt":"2026-07-14T08:00:00.000Z","messageCount":6,"summaryAnchorMessageIds":ids(7)[:4]},
 {"id":K[8],"participants":seats,"messages":M[8],"lastMessageAt":"2026-07-14T07:00:00.000Z","messageCount":6,"summaryAnchorMessageIds":ids(8)[:4]},
]
Z={"promptTokens":0,"completionTokens":0,"totalTokens":0}
def ep(n,summary,imp=0.7):
    return {"narrative":n,"summary":summary,"when":"2026-07-14","entities":["Gull"],"importance":imp}
rules=[
 {"pass":"episode","contains":"K030 ","response":json.dumps([ep("On July 14th Ada and Bram set out aboard the Gull.","set out aboard the gull")]),"usage":Z},
 {"pass":"episode","response":"[]","usage":Z},
 {"pass":"other","label":"Ada (she/her/her)","response":json.dumps([{"subjectIndex":1,"content":"Bram keeps the ship's ledger.","summary":"bram keeps the ledger","importance":0.7}]),"usage":Z},
 {"pass":"other","label":"Bram (he/him/his)","response":"[]","usage":Z},
]
N="2026-07-14T14:00:00.000Z"
job=lambda **kw:{"turnOpenerMessageId":None,"extractionAnchorMessageId":None,"connectionProfileId":P_CUR,**kw}
ops=[
 {"name":"sel_other","kind":"select","pass":"other-fold","now":N,"plants":{"cursors":{K[8]:{"other-fold":ids(8)[-1],"episode-fold":ids(8)[3]}}}},
 {"name":"sel_episode","kind":"select","pass":"episode-fold","now":N},
 {"name":"sel_other_limit_one","kind":"select","pass":"other-fold","now":N,"limit":1},
 {"name":"sel_other_mode_turn","kind":"select","pass":"other-fold","now":N,"plants":{"mode":{"otherPass":"turn","perTurnOtherFloor":0.75,"foldCandidatesPerSubject":3}}},
 {"name":"sweep_other","kind":"sweep","pass":"other-fold","now":N},
 {"name":"sweep_other_in_flight","kind":"sweep","pass":"other-fold","now":N},
 {"name":"sweep_episode_capped","kind":"sweep","pass":"episode-fold","now":N,"limit":0},
 {"name":"sweep_episode","kind":"sweep","pass":"episode-fold","now":N},
 {"name":"catchup_never_folded","kind":"catchup","chatId":K[2],"pass":"episode-fold","now":N},
 {"name":"catchup_not_idle","kind":"catchup","chatId":K[3],"pass":"other-fold","now":N},
 {"name":"catchup_other_ignore_idle","kind":"catchup","chatId":K[3],"pass":"other-fold","now":N,"ignoreIdle":True},
 {"name":"catchup_other_behind_cursor","kind":"catchup","chatId":K[3],"pass":"other-fold","now":N,"ignoreIdle":True},
 {"name":"catchup_episode_behind_cursor","kind":"catchup","chatId":K[8],"pass":"episode-fold","now":N},
 {"name":"handler_other_continues","kind":"handler","chatId":K[1],"payload":job(chatId=K[1],**{"pass":"other-fold"})},
 {"name":"handler_other_continuation","kind":"handler","chatId":K[1],"payload":job(chatId=K[1],**{"pass":"other-fold","cursor":ids(1)[239]})},
 {"name":"handler_episode_legacy_flag","kind":"handler","chatId":K[1],"payload":job(chatId=K[1],foldEpisodeCatchup=True)},
 {"name":"handler_not_idle","kind":"handler","chatId":K[3],"plants":{"lastMessageAtNow":True,"cursors":{K[3]:{"other-fold":None}}},"payload":job(chatId=K[3],**{"pass":"other-fold"})},
 {"name":"handler_legacy_ignore_idle","kind":"handler","chatId":K[3],"plants":{"lastMessageAtNow":True},"payload":job(chatId=K[3],foldOtherCatchup=True,foldOtherIgnoreIdle=True)},
 {"name":"handler_timeout_fails_the_job","kind":"handler","chatId":K[2],"plants":{"failContains":"B005 "},"payload":job(chatId=K[2],**{"pass":"other-fold"})},
 {"name":"handler_bogus_first","kind":"handler","chatId":K[1],"payload":job(chatId=K[1],**{"pass":"bogus"})},
 {"name":"handler_bogus_second","kind":"handler","chatId":K[1],"payload":job(chatId=K[1],**{"pass":"bogus"})},
 {"name":"handler_no_chat","kind":"handler","chatId":"c9000000-0000-4000-8000-0000000000c9","payload":job(chatId="c9000000-0000-4000-8000-0000000000c9",**{"pass":"other-fold"})},
]
spec={
 "testPepperBase64":"dGVzdC1wZXBwZXItZm9yLWZpeHR1cmVzLW9ubHkxMjM0NTY3OA==",
 "userId":U,"seedTimestamp":"2026-07-01T00:00:00.000Z","futureTimestamp":"2026-07-01T00:00:00.000Z",
 "connectionProfiles":[
   {"id":P_CUR,"name":"Current","provider":"ANTHROPIC","modelName":"claude-sonnet-4-5","isCheap":False,"isDangerousCompatible":False},
   {"id":P_CHEAP,"name":"Cheap","provider":"OPENAI","modelName":"gpt-4o-mini","isCheap":True,"isDangerousCompatible":False}],
 "chatSettings":{"cheapLLMSettings":{"strategy":"PROVIDER_CHEAPEST","fallbackToLocal":False}},
 "characters":[
   {"id":ADA,"name":"Ada","controlledBy":"llm","identity":"Ada captains the Gull.","pronouns":{"subject":"she","object":"her","possessive":"her"},"othersFiles":[]},
   {"id":BRAM,"name":"Bram","controlledBy":"llm","identity":"Bram is the quartermaster.","pronouns":{"subject":"he","object":"him","possessive":"his"},"othersFiles":[]}],
 "seedMemories":[],
 "chats":chats,
 "embeddingDims":16,
 "completionRules":rules,
 "unknownPassChatId":K[1],
 "ops":ops,
}
json.dump(spec,open(OUT,'w'),indent=1,ensure_ascii=False)
print(len(ops))
