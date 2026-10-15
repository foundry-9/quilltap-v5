#!/usr/bin/env python3
"""Generate harness/oracle/fixtures/memory-processor-tier3.json (work order P4.D296).

The spec the shared `build-memory-processor-fixture.ts` builds from (with
`QT_FIXTURE_SPEC=memory-processor-tier3.json` when it is not the default) and the oracle case reads.
Run: python3 harness/oracle/scripts/gen-memory-processor-spec.py — then rebuild the fixture pair and
regenerate the oracle (the Rust family's header has the recipe).
"""
import os
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'fixtures', 'memory-processor-tier3.json')
import json
U='a0000000-0000-4000-8000-0000000000e1'
ADA='aa000000-0000-4000-8000-0000000000a1'; BRAM='bb000000-0000-4000-8000-0000000000b2'; QUILL='cc000000-0000-4000-8000-0000000000c3'
P_CUR='9c000000-0000-4000-8000-000000000001'; P_CHEAP='9c000000-0000-4000-8000-000000000002'; P_DEF='9c000000-0000-4000-8000-000000000003'
CH_A='c4a00000-0000-4000-8000-00000000000a'; CH_B='c4b00000-0000-4000-8000-00000000000b'
PA='5a000000-0000-4000-8000-00000000000a'; PB='5b000000-0000-4000-8000-00000000000b'; PQ='5c000000-0000-4000-8000-00000000000c'
def m(i,role,pid,content,chat='a'):
    return {"id":f"{chat}{i:07d}-0000-4000-8000-000000000000"[:36] if False else f"e{chat}{i:06d}-0000-4000-8000-000000000{i:03d}","role":role,"content":content,"participantId":pid,"createdAt":f"2026-07-14T09:{i:02d}:00.000Z"}
A=[m(1,'USER',PQ,'I told her we sail at dawn, whatever the harbourmaster says.'),
   m(2,'ASSISTANT',PA,'Then I will ready the Gull tonight and post the watch myself.'),
   m(3,'ASSISTANT',PB,'I will bargain with the harbourmaster for the berth fee.'),
   m(4,'ASSISTANT',PA,'And I will chart the reef passage before the tide turns.'),
   m(5,'USER',PQ,'Good. Bram, keep the brass sextant safe.'),
   m(6,'ASSISTANT',PB,'It never leaves my coat until we reach Lighthouse Point.')]
B=[m(1,'ASSISTANT',PA,'The lamps are lit along the quay.','b'),
   m(2,'ASSISTANT',PB,'I counted nine ships in the harbour tonight.','b'),
   m(3,'ASSISTANT',PA,'Nine? I counted ten, with the black brig.','b')]
ids=lambda L:[x['id'] for x in L]
print(ids(A),ids(B))
def self_resp(*items): return json.dumps(list(items))
def item(content,summary,imp,**kw):
    d={"content":content,"summary":summary,"keywords":["voyage"],"importance":imp,"temporal":"future","scope":"narrow","context":"history"}; d.update(kw); return d
rules=[
 # SELF per subject label
 {"pass":"self","label":"Ada (she/her/her)","contains":"reef passage","response":self_resp(item("I promised to chart the reef passage before the tide turned.","promised to chart the reef",0.8),item("I chose to post the watch myself.","posted the watch myself",0.55)),"usage":{"promptTokens":900,"completionTokens":60,"totalTokens":960}},
 {"pass":"self","label":"Bram (he/him/his)","contains":"berth fee","response":self_resp(item("I took on bargaining for the berth fee.","bargained for the berth",0.72,kind="episodic",when="2026-07-14",entities=["harbourmaster"])),"usage":{"promptTokens":880,"completionTokens":50,"totalTokens":930}},
 {"pass":"self","label":"Quill","response":self_resp(item("I insisted we sail at dawn whatever the harbourmaster said.","insisted on a dawn sailing",0.9)),"usage":{"promptTokens":870,"completionTokens":40,"totalTokens":910}},
 {"pass":"self","label":"Bram (he/him/his)","contains":"sextant","response":self_resp(item("I swore to keep the brass sextant in my coat.","kept the sextant safe",0.0)),"usage":{"promptTokens":800,"completionTokens":30,"totalTokens":830}},
 {"pass":"self","label":"Ada (she/her/her)","contains":"black brig","response":self_resp(item("I disputed the ship count, insisting on ten.","disputed the ship count",0.6)),"usage":{"promptTokens":600,"completionTokens":20,"totalTokens":620}},
 {"pass":"self","label":"Bram (he/him/his)","contains":"nine ships","response":"[]","usage":{"promptTokens":600,"completionTokens":2,"totalTokens":602}},
 # OTHER per observer label
 {"pass":"other","label":"Ada (she/her/her)","response":json.dumps([{"subjectIndex":1,"content":"Bram took the berth bargaining on himself.","summary":"bram took the bargaining","importance":0.8},{"subjectIndex":2,"content":"Quill wanted a dawn sailing regardless of the harbourmaster.","summary":"quill wants dawn sailing","importance":0.6}]),"usage":{"promptTokens":1000,"completionTokens":80,"totalTokens":1080}},
 {"pass":"other","label":"Bram (he/him/his)","response":json.dumps([{"subjectIndex":1,"content":"Ada means to chart the reef passage herself.","summary":"ada charts the reef","importance":0.9},{"subjectIndex":2,"content":"Quill trusts me with the sextant.","summary":"quill trusts me","importance":0.77}]),"usage":{"promptTokens":990,"completionTokens":70,"totalTokens":1060}},
 {"pass":"other","label":"Quill","response":json.dumps([{"subjectIndex":1,"content":"Ada readied the Gull without being asked.","summary":"ada readied the gull","importance":0.85}]),"usage":{"promptTokens":950,"completionTokens":40,"totalTokens":990}},
]
spec={
 "testPepperBase64":"dGVzdC1wZXBwZXItZm9yLWZpeHR1cmVzLW9ubHkxMjM0NTY3OA==",
 "userId":U,"seedTimestamp":"2026-07-01T00:00:00.000Z","futureTimestamp":"2026-07-01T00:00:00.000Z",
 "connectionProfiles":[
   {"id":P_CUR,"name":"Current","provider":"ANTHROPIC","modelName":"claude-sonnet-4-5","isCheap":False,"isDangerousCompatible":False},
   {"id":P_CHEAP,"name":"Cheap","provider":"OPENAI","modelName":"gpt-4o-mini","isCheap":True,"isDangerousCompatible":False},
   {"id":P_DEF,"name":"Default cheap","provider":"GOOGLE","modelName":"gemini-2.5-flash","isCheap":False,"isDangerousCompatible":False}],
 "chatSettings":{"cheapLLMSettings":{"strategy":"PROVIDER_CHEAPEST","fallbackToLocal":False}},
 "characters":[
   {"id":ADA,"name":"Ada","controlledBy":"llm","identity":"Ada captains the Gull.","pronouns":{"subject":"she","object":"her","possessive":"her"},"othersFiles":[]},
   {"id":BRAM,"name":"Bram","controlledBy":"llm","identity":"Bram is the Gull's quartermaster.","pronouns":{"subject":"he","object":"him","possessive":"his"},"othersFiles":[]},
   {"id":QUILL,"name":"Quill","controlledBy":"user","identity":"Quill is a scribe.","othersFiles":[]}],
 "seedMemories":[],
 "chats":[
   {"id":CH_A,"participants":[{"id":PA,"characterId":ADA,"controlledBy":"llm","connectionProfileId":P_CUR},{"id":PB,"characterId":BRAM,"controlledBy":"llm","connectionProfileId":P_CUR},{"id":PQ,"characterId":QUILL,"controlledBy":"user"}],"messages":A},
   {"id":CH_B,"chatType":"autonomous","participants":[{"id":PA,"characterId":ADA,"controlledBy":"llm","connectionProfileId":P_CUR},{"id":PB,"characterId":BRAM,"controlledBy":"llm","connectionProfileId":P_CUR}],"messages":B}],
 "embeddingDims":16,
 "completionRules":rules,
 "ops":[
  {"name":"turn3_full_admission_throw","chatId":CH_A,"plants":{"failContent":"I took on bargaining for the berth fee."},"locator":{"turnOpenerMessageId":A[0]['id']}},
  {"name":"turn3_reclose_same_turn","chatId":CH_A,"locator":{"turnOpenerMessageId":A[0]['id']}},
  {"name":"turn3_reopen_mid_turn","chatId":CH_A,"plants":{"cursor":A[1]['id']},"locator":{"turnOpenerMessageId":A[0]['id']}},
  {"name":"turn2_foreign_cursor_zero_importance","chatId":CH_A,"plants":{"cursor":A[2]['id']},"locator":{"turnOpenerMessageId":A[4]['id']}},
  {"name":"turn1_backfill_forward_only","chatId":CH_A,"opts":{"backfill":True},"locator":{"turnOpenerMessageId":A[0]['id']}},
  {"name":"autonomous_bound","chatId":CH_B,"locator":{"turnOpenerMessageId":None,"extractionAnchorMessageId":B[2]['id'],"extractAfterMessageId":B[1]['id']}},
  {"name":"mode_fold","chatId":CH_A,"plants":{"cursor":None,"mode":{"otherPass":"fold","perTurnOtherFloor":0.75,"foldCandidatesPerSubject":3}},"locator":{"turnOpenerMessageId":A[0]['id']}},
  {"name":"mode_turn_default_cheap","chatId":CH_A,"plants":{"cursor":None,"mode":{"otherPass":"turn","perTurnOtherFloor":0.75,"foldCandidatesPerSubject":3},"cheapLLMSettings":{"strategy":"PROVIDER_CHEAPEST","fallbackToLocal":False,"defaultCheapProfileId":P_DEF}},"locator":{"turnOpenerMessageId":A[0]['id']}},
  {"name":"throttle_and_skip","chatId":CH_A,"plants":{"cursor":None,"limits":{"enabled":True,"maxPerHour":4,"softStartFraction":0.5,"softFloor":0.7},"ledger":{ADA:4,BRAM:2}},"locator":{"turnOpenerMessageId":A[0]['id']}},
  {"name":"backfill_ignores_limits","chatId":CH_A,"opts":{"backfill":True},"plants":{"cursor":None,"limits":{"enabled":True,"maxPerHour":4,"softStartFraction":0.5,"softFloor":0.7}},"locator":{"turnOpenerMessageId":A[0]['id']}},
  {"name":"timeout_self_dry_run","chatId":CH_B,"opts":{"dryRun":True},"plants":{"failLabel":"Ada (she/her/her)"},"locator":{"turnOpenerMessageId":None,"extractionAnchorMessageId":B[2]['id']}},
 ],
}
json.dump(spec,open(OUT,'w'),indent=1,ensure_ascii=False)
