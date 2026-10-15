#!/usr/bin/env python3
"""Generate harness/oracle/fixtures/fold-episode-tier3.json (work order P4.D296).

The spec the shared `build-memory-processor-fixture.ts` builds from (with
`QT_FIXTURE_SPEC=fold-episode-tier3.json` when it is not the default) and the oracle case reads.
Run: python3 harness/oracle/scripts/gen-fold-episode-spec.py — then rebuild the fixture pair and
regenerate the oracle (the Rust family's header has the recipe).
"""
import os
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'fixtures', 'fold-episode-tier3.json')
import json
U='a0000000-0000-4000-8000-0000000000e1'
ADA='aa000000-0000-4000-8000-0000000000a1'; BRAM='bb000000-0000-4000-8000-0000000000b2'; QUILL='cc000000-0000-4000-8000-0000000000c3'
P_CUR='9c000000-0000-4000-8000-000000000001'; P_CHEAP='9c000000-0000-4000-8000-000000000002'
F1='f1000000-0000-4000-8000-0000000000f1'; F2='f2000000-0000-4000-8000-0000000000f2'; F3='f3000000-0000-4000-8000-0000000000f3'
PA='5a000000-0000-4000-8000-00000000000a'; PB='5b000000-0000-4000-8000-00000000000b'; PQ='5c000000-0000-4000-8000-00000000000c'
def mid(chat,i): return f"{chat[:2]}{i:06d}-0000-4000-8000-{i:012d}"
def ts(i): 
    h,mn=divmod(i,60); return f"2026-07-14T{8+h:02d}:{mn:02d}:00.000Z"
F1M=[]
for i in range(1,301):
    sp=PA if i%2 else PB
    m={"id":mid('d1',i),"role":"ASSISTANT","content":f"L{i:03d} the voyage continues at stop {i}.","participantId":sp,"createdAt":ts(i)}
    F1M.append(m)
# ineligible rows: a staff whisper and a blank line
F1M[9]["content"]="   "            # blank — not eligible
F1M[19]={"id":mid('d1',20),"role":"USER","content":"L020 a line without a seat","participantId":None,"createdAt":ts(20)}
F2M=[{"id":mid('d2',i),"role":"ASSISTANT","content":f"P{i:03d} at the harbour with Quill.","participantId":PA if i%2 else PQ,"createdAt":ts(i)} for i in range(1,9)]
F3M=[{"id":mid('d3',i),"role":"ASSISTANT","content":f"Q{i:03d} nobody is here.","participantId":PA,"createdAt":ts(i)} for i in range(1,5)]
def ep(n,summary,imp=0.7,**kw):
    d={"narrative":n,"summary":summary,"when":"2026-07-14","entities":["Gull","Lighthouse Point"],"importance":imp}; d.update(kw); return d
rules=[
 {"pass":"episode","contains":"L030 ","response":json.dumps([ep("On July 14th Ada and Bram began the voyage aboard the Gull.","began the voyage"),ep("On July 14th Bram stowed the brass sextant at Lighthouse Point.","stowed the sextant",0.55)]),"usage":{"promptTokens":3000,"completionTokens":90,"totalTokens":3090}},
 {"pass":"episode","contains":"L090 ","response":json.dumps([ep("On July 14th the Gull rounded the reef by noon.","rounded the reef")]),"usage":{"promptTokens":3000,"completionTokens":40,"totalTokens":3040}},
 {"pass":"episode","contains":"L130 ","response":json.dumps([ep("On July 14th Ada and Bram began the voyage aboard the Gull.","began the voyage")]),"usage":{"promptTokens":3000,"completionTokens":40,"totalTokens":3040}},
 {"pass":"episode","contains":"L280 ","response":json.dumps([ep("On July 14th the Gull dropped anchor in the cove at nightfall.","dropped anchor in the cove",0.6)]),"usage":{"promptTokens":3000,"completionTokens":40,"totalTokens":3040}},
 {"pass":"episode","contains":"L230 ","response":json.dumps([ep("On July 14th the crew sighted Lighthouse Point at dusk.","sighted the lighthouse",0.9)]),"usage":{"promptTokens":2000,"completionTokens":40,"totalTokens":2040}},
 {"pass":"episode","contains":"P001 ","response":json.dumps([ep("On July 14th Ada met Quill at the harbour.","met quill at the harbour")]),"usage":{"promptTokens":500,"completionTokens":30,"totalTokens":530}},
 {"pass":"episode","response":"[]","usage":{"promptTokens":100,"completionTokens":2,"totalTokens":102}},
 {"pass":"other","label":"Ada (she/her/her)","contains":"L030 ","response":json.dumps([{"subjectIndex":1,"content":"Bram steered the reef passage well.","summary":"bram steered the reef","importance":0.8}]),"usage":{"promptTokens":2500,"completionTokens":40,"totalTokens":2540}},
 {"pass":"other","label":"Ada (she/her/her)","response":json.dumps([{"subjectIndex":1,"content":"Bram kept a steady watch all day.","summary":"bram kept watch","importance":0.7},{"subjectIndex":1,"content":"Bram hums sea shanties.","summary":"bram hums shanties","importance":0.5}]),"usage":{"promptTokens":2500,"completionTokens":50,"totalTokens":2550}},
 {"pass":"other","label":"Bram (he/him/his)","response":json.dumps([{"subjectIndex":1,"content":"Ada charted every stop of the voyage.","summary":"ada charted the voyage","importance":0.75}]),"usage":{"promptTokens":2500,"completionTokens":40,"totalTokens":2540}},
 {"pass":"other","label":"Quill","response":"[]","usage":{"promptTokens":200,"completionTokens":2,"totalTokens":202}},
]
seeds=[
 {"id":"5eed0001-0000-4000-8000-000000000001","characterId":ADA,"chatId":F1,"content":"Ada noted the first stop.","summary":"noted the first stop","vector":None,"createdAt":"seed","sourceMessageId":mid('d1',1)},
 {"id":"5eed0002-0000-4000-8000-000000000002","characterId":ADA,"chatId":F1,"content":"Ada noted the fifth stop.","summary":"noted the fifth stop","vector":None,"createdAt":"seed","sourceMessageId":mid('d1',5)},
 {"id":"5eed0003-0000-4000-8000-000000000003","characterId":BRAM,"chatId":F1,"content":"Bram noted the second stop.","summary":"noted the second stop","vector":None,"createdAt":"seed","sourceMessageId":mid('d1',2)},
]
W=lambda L,a,b:[x['id'] for x in L[a:b]]
spec={
 "testPepperBase64":"dGVzdC1wZXBwZXItZm9yLWZpeHR1cmVzLW9ubHkxMjM0NTY3OA==",
 "userId":U,"seedTimestamp":"2026-07-01T00:00:00.000Z","futureTimestamp":"2026-07-01T00:00:00.000Z",
 "connectionProfiles":[
   {"id":P_CUR,"name":"Current","provider":"ANTHROPIC","modelName":"claude-sonnet-4-5","isCheap":False,"isDangerousCompatible":False},
   {"id":P_CHEAP,"name":"Cheap","provider":"OPENAI","modelName":"gpt-4o-mini","isCheap":True,"isDangerousCompatible":False}],
 "chatSettings":{"cheapLLMSettings":{"strategy":"PROVIDER_CHEAPEST","fallbackToLocal":False}},
 "characters":[
   {"id":ADA,"name":"Ada","controlledBy":"llm","identity":"Ada captains the Gull.","pronouns":{"subject":"she","object":"her","possessive":"her"},"othersFiles":[]},
   {"id":BRAM,"name":"Bram","controlledBy":"llm","identity":"Bram is the quartermaster.","pronouns":{"subject":"he","object":"him","possessive":"his"},"othersFiles":[]},
   {"id":QUILL,"name":"Quill","controlledBy":"user","identity":"Quill is a scribe.","othersFiles":[]}],
 "seedMemories":seeds,
 "chats":[
   {"id":F1,"participants":[{"id":PA,"characterId":ADA,"controlledBy":"llm","connectionProfileId":P_CUR},{"id":PB,"characterId":BRAM,"controlledBy":"llm","connectionProfileId":P_CUR}],"messages":F1M},
   {"id":F2,"participants":[{"id":PA,"characterId":ADA,"controlledBy":"llm","connectionProfileId":P_CUR},{"id":PB,"characterId":BRAM,"controlledBy":"llm","connectionProfileId":P_CUR},{"id":PQ,"characterId":QUILL,"controlledBy":"user"}],"statuses":{PB:"absent"},"messages":F2M},
   {"id":F3,"participants":[{"id":PA,"characterId":ADA,"controlledBy":"llm","connectionProfileId":P_CUR}],"statuses":{PA:"removed"},"messages":F3M}],
 "embeddingDims":16,
 "completionRules":rules,
 "ops":[
  {"name":"ep_run1_two_slices_of_three","chatId":F1,"pass":"episode-fold","window":[0,150],"maxSlices":2},
  {"name":"ep_run2_reads_the_rest","chatId":F1,"pass":"episode-fold","window":[0,150],"maxSlices":2},
  {"name":"ep_behind_cursor","chatId":F1,"pass":"episode-fold","window":[0,150]},
  {"name":"ep_reinforce_not_linked_two_slices","chatId":F1,"pass":"episode-fold","window":[150,300],"plants":{"cursor":{"episode-fold":mid('d1',150)}}},
  {"name":"other_timeout_on_slice_2","chatId":F1,"pass":"other-fold","window":[0,300],"plants":{"failContains":"L090 "}},
  {"name":"other_resume_reads_the_gap","chatId":F1,"pass":"other-fold","window":[290,300],"maxSlices":1},
  {"name":"other_mode_turn","chatId":F1,"pass":"other-fold","window":[0,10],"plants":{"mode":{"otherPass":"turn","perTurnOtherFloor":0.75,"foldCandidatesPerSubject":3}}},
  {"name":"empty_window","chatId":F1,"pass":"episode-fold","window":[0,0]},
  {"name":"nothing_to_observe_advances","chatId":F2,"pass":"other-fold","windowIds":[],"windowRaw":[{"type":"message","id":"aa00ff01-0000-4000-8000-000000000001","role":"ASSISTANT","content":"[Host] note","participantId":None,"systemSender":"host","createdAt":"2026-07-14T09:00:00.000Z"}]},
  {"name":"ep_partially_capped_holders","chatId":F2,"pass":"episode-fold","window":[0,8],"plants":{"limits":{"enabled":True,"maxPerHour":1,"softStartFraction":0.5,"softFloor":0.7}}},
  {"name":"ep_rate_deferred_every_holder_capped","chatId":F2,"pass":"episode-fold","window":[0,8],"plants":{"cursor":{"episode-fold":None},"limits":{"enabled":True,"maxPerHour":1,"softStartFraction":0.5,"softFloor":0.7}}},
  {"name":"ep_partially_present_holders","chatId":F2,"pass":"episode-fold","window":[0,8],"plants":{"cursor":{"episode-fold":None}}},
  {"name":"ep_nobody_present","chatId":F3,"pass":"episode-fold","window":[0,4]},
  {"name":"ep_dry_run_reads_all","chatId":F1,"pass":"episode-fold","window":[0,300],"opts":{"dryRun":True},"maxSlices":"Infinity"},
  {"name":"bogus_pass_runs_the_episode_branch","chatId":F2,"pass":"bogus","window":[0,8]},
 ],
}
json.dump(spec,open(OUT,'w'),indent=1,ensure_ascii=False)
print(len(F1M), mid('d1',150))
