#!/usr/bin/env python3
"""Generate `harness/oracle/fixtures/fold-other-pass-tier3.json` — the corpus of
the `fold_other_pass_tier3` family (P4.D270, v4 `d58548051` at pin `01a83539d`).

Usage: gen-fold-other-pass-spec.py harness/oracle/fixtures/fold-other-pass-tier3.json

The chats, completion rules and ops are built here rather than by hand: ids are
UUID-shaped by construction, every completion response names its subject (the
gate's name-presence check), and the catch-up chats' `lastMessageAt` values are
computed against the corpus's FIXED `now`. Regenerate the builder's fixture and
the oracle after any change (the family header's recipe).
"""
import json, collections
O=collections.OrderedDict
USER='00000000-0000-4000-8000-0000000000ee'
PROF='e0000000-0000-4000-8000-0000000000c0'
CHEAP='e0000000-0000-4000-8000-0000000000c1'
C=O(ada='fa000000-0000-4000-8000-00000000000a', bram='fb000000-0000-4000-8000-00000000000b',
    cass='fc000000-0000-4000-8000-00000000000c', dov='fd000000-0000-4000-8000-00000000000d',
    eve='fe000000-0000-4000-8000-00000000000e', fay='ff000000-0000-4000-8000-00000000000f')
NAMES=dict(ada='Ada',bram='Bram',cass='Cass',dov='Dov',eve='Eve',fay='Fay')
PRON=dict(ada=('she','her','her'),bram=('he','him','his'),cass=('they','them','their'),dov=('he','him','his'),eve=('she','her','her'),fay=('she','her','her'))
def chat_id(n): return f'c{n:07x}-0000-4000-8000-000000000000'
def seat_id(n,s): return f'b{n:03x}{s:04x}-0000-4000-8000-000000000000'
def msg_id(n,m): return f'e{n:03x}{m:04x}-0000-4000-8000-{n:04x}{m:08x}'
FIXED_NOW='2026-09-01T12:00:00.000Z'
OLD='2026-01-05T10:00:00.000Z'
def ts(base_min, i):
    h=10+(base_min+i)//60; mm=(base_min+i)%60
    return f'2026-01-05T{h:02d}:{mm:02d}:00.000Z'
chats=[]
def mk_chat(n, seats, script, chat_type=None, timeline=None, last=OLD, count=None, watermark=None, tag=None, extra_msgs=None):
    # seats: list of (key, controlledBy, profile or None)
    parts=[]
    for i,(k,cb,prof) in enumerate(seats):
        p=O(id=seat_id(n,i+1),characterId=C[k],controlledBy=cb)
        if prof: p['connectionProfileId']=prof
        parts.append(p)
    msgs=[]
    for i,row in enumerate(script):
        k=row[0]; text=row[1]; over=row[2] if len(row)>2 else {}
        seat=next((p for p,(kk,_,_) in zip(parts,seats) if kk==k),None)
        m=O(id=msg_id(n,i+1), role=('USER' if (seat and next(cb for kk,cb,_ in seats if kk==k)=='user') else 'ASSISTANT'),
            content=text, participantId=(seat['id'] if seat else None), createdAt=ts(n,i))
        m.update(over)
        msgs.append(m)
    c=O(id=chat_id(n), title=f'Fold OTHER chat {n}', participants=parts, messages=msgs)
    if chat_type: c['chatType']=chat_type
    if timeline: c['timelineMode']=timeline
    c['lastMessageAt']=last
    c['messageCount']=count if count is not None else len(msgs)
    if watermark: c['otherExtractionWatermarkMessageId']=watermark
    if tag: c['contextSummary']=f'[op:{tag}] The household has been talking for a while.'
    chats.append(c)
    return c
AB=[('ada','llm',PROF),('bram','llm',None)]
def alt(n,count,a='ada',b='bram'):
    return [((a if i%2==0 else b), f'Line {i+1} of chat {n}: the plan for the harbor festival moves on.') for i in range(count)]
# --- direct-pass chats ---
mk_chat(1, AB, alt(1,4))
mk_chat(2, AB+[('cass','user',None)], alt(2,6))
mk_chat(3, AB, alt(3,4))
mk_chat(4, AB, alt(4,6), watermark=msg_id(4,3))
mk_chat(5, AB, alt(5,4), watermark='e0ffffff-0000-4000-8000-0000ffffffff')
mk_chat(6, AB+[('cass','user',None)], [
  ('ada','Ada opens the ledger and reads the harbor tally aloud.'),
  ('bram','Bram says the tally is short by three crates.'),
  ('ada','The Lantern notes the hour.', {'systemSender':'lantern'}),
  ('none','A line with no seat attached.', {'role':'USER'}),
  ('bram','Bram promises to recount the crates at dawn.'),
  ('none','{"tool":"tally","result":3}', {'role':'TOOL'}),
])
mk_chat(7, AB, alt(7,70))
mk_chat(8, [('ada','llm',PROF)], alt(8,2,'ada','ada'))
mk_chat(9, [('eve','llm',PROF),('ada','llm',None)], alt(9,4,'eve','ada'))
mk_chat(10, [('fay','llm',PROF),('ada','llm',None)], alt(10,4,'fay','ada'))
mk_chat(11, AB, alt(11,4))
mk_chat(12, AB, alt(12,4))
mk_chat(13, [('dov','llm',PROF),('ada','llm',None)], alt(13,4,'dov','ada'))
mk_chat(14, AB, alt(14,4), chat_type='autonomous')
mk_chat(15, AB, alt(15,4), timeline='narrative')
mk_chat(16, AB, alt(16,4))
mk_chat(17, AB, alt(17,4), chat_type='help')
# --- catch-up chats (relative to FIXED_NOW = 2026-09-01T12:00Z) ---
def rel(hours=0, days=0):
    import datetime
    t=datetime.datetime(2026,9,1,12,0,0)-datetime.timedelta(hours=hours,days=days)
    return t.strftime('%Y-%m-%dT%H:%M:%S.000Z')
mk_chat(20, AB, alt(20,4), last=rel(hours=3), tag='c20')
mk_chat(21, AB, alt(21,4), last=rel(hours=1), tag='c21')
mk_chat(22, AB, alt(22,4), last=rel(days=31), tag='c22')
mk_chat(23, AB, alt(23,4), last=rel(hours=2), tag='c23')
mk_chat(24, AB, alt(24,4), chat_type='help', last=rel(hours=3), tag='c24')
mk_chat(25, AB, alt(25,1), last=rel(hours=3), count=1, tag='c25')
mk_chat(26, AB, alt(26,4), last=rel(hours=3), watermark=msg_id(26,4), tag='c26')
mk_chat(27, [('ada','llm',None),('bram','llm',None)], alt(27,4), last=rel(hours=3), tag='c27')
mk_chat(28, [('cass','user',CHEAP),('ada','llm',None),('bram','llm',None)], alt(28,4), last=rel(hours=5), tag='c28')
mk_chat(29, [('cass','user',CHEAP),('ada','llm',PROF),('bram','llm',None)], alt(29,4), last=rel(hours=4), tag='c29')
mk_chat(30, AB, alt(30,4), last=rel(days=40), tag='c30')
# --- completion rules: per (op tag, observer) ---
def subjects_for(chat, observer_key, user_key=None):
    return None
rules=[]
def candidates(op, obs, subj_names, narrative=False, lows=True):
    out=[]
    for i,s in enumerate(subj_names, start=1):
        base=[(0.9,'semantic'),(0.6,'semantic')]+([(0.4,'semantic')] if lows else [])+[(0.5,'episodic')]
        for j,(imp,kind) in enumerate(base):
            c=O(subjectIndex=i, content=f'[{op}] {obs} noticed that {s} handled the harbor task {j+1}.', summary=f'{op} {s} task {j+1}', keywords=['harbor',op], importance=imp)
            if kind=='episodic':
                c['kind']='episodic'; c['entities']=['Harbor Gate']
                if narrative: c['when']='the third evening of the festival'
            out.append(c)
    return json.dumps(out)
USAGE=O(promptTokens=200,completionTokens=40,totalTokens=240)
def rule(op, obs, subjects, error=None, narrative=False, lows=True):
    r=O(op=op, observer=obs, response=candidates(op,obs,subjects,narrative,lows), usage=USAGE)
    if error: r['error']=error
    rules.append(r)
for op,pairs in [
  ('turn_mode',[]),
  ('hybrid_two',[('Ada',['Bram','Cass']),('Bram',['Ada','Cass'])]),
  ('fold_cap1',[('Ada',['Bram']),('Bram',['Ada'])]),
  ('wm_mid',[('Ada',['Bram']),('Bram',['Ada'])]),
  ('wm_vanished',[('Ada',['Bram']),('Bram',['Ada'])]),
  ('ineligible',[('Ada',['Bram','Cass']),('Bram',['Ada','Cass'])]),
  ('window70',[('Ada',['Bram']),('Bram',['Ada'])]),
  ('rate_limited',[('Ada',['Eve'])]),
  ('throttled',[('Fay',['Ada']),('Ada',['Fay'])]),
  ('vault_canon',[('Dov',['Ada']),('Ada',['Dov'])]),
  ('autonomous',[('Ada',['Bram']),('Bram',['Ada'])]),
  ('help_chat',[('Ada',['Bram']),('Bram',['Ada'])]),
  ('c20',[('Ada',['Bram']),('Bram',['Ada'])]),
  ('c28',[('Ada',['Bram','Cass']),('Bram',['Ada','Cass'])]),
  ('c29',[('Ada',['Bram','Cass']),('Bram',['Ada','Cass'])]),
]:
    for obs,subs in pairs: rule(op,obs,subs)
rule('timeout','Ada',['Bram']); rule('timeout','Bram',['Ada'],error='Request timed out.')
rule('refused','Ada',['Bram'],error='The model declined to answer.'); rule('refused','Bram',['Ada'])
rule('narrative','Ada',['Bram'],narrative=True); rule('narrative','Bram',['Ada'],narrative=True)
rule('gate_fail','Ada',['Bram']); rule('gate_fail','Bram',['Ada'])
rule('c30','Ada',['Bram']); rule('c30','Bram',['Ada'],error='Request timed out.')
LIMITS=O(enabled=True,maxPerHour=4,softStartFraction=0.5,softFloor=0.55)
def pass_op(name, chat_n, tag, **kw):
    o=O(op='pass', name=name, chatId=chat_id(chat_n), tag=tag)
    o.update(kw); return o
ops=[
 pass_op('turn_mode',1,'turn_mode',mode=O(otherPass='turn')),
 pass_op('hybrid_two_observers_silent_user',2,'hybrid_two'),
 pass_op('fold_mode_cap1',3,'fold_cap1',mode=O(otherPass='fold',foldCandidatesPerSubject=1)),
 pass_op('behind_watermark',2,'hybrid_two'),
 pass_op('watermark_mid_window',4,'wm_mid'),
 pass_op('vanished_watermark',5,'wm_vanished'),
 pass_op('ineligible_rows_trailing',6,'ineligible'),
 pass_op('window_70_trimmed',7,'window70'),
 pass_op('nothing_to_observe',8,'nothing'),
 pass_op('rate_limited_observer',9,'rate_limited',limits=LIMITS),
 pass_op('throttled_observer',10,'throttled',limits=LIMITS),
 pass_op('observer_timeout',11,'timeout'),
 pass_op('observer_refused',12,'refused'),
 pass_op('vault_canon',13,'vault_canon'),
 pass_op('autonomous_room',14,'autonomous',inAutonomousRoom=True),
 pass_op('narrative_timeline',15,'narrative',timelineMode='narrative'),
 pass_op('gate_write_failure',16,'gate_fail'),
 pass_op('help_chat',17,'help_chat'),
 O(op='find',name='find_all',limit=50),
 O(op='find',name='find_limit_1',limit=1),
 O(op='find',name='find_turn_mode',limit=50,mode=O(otherPass='turn')),
 O(op='enqueue',name='enqueue_beside_per_turn_job',limit=None,priorTurnJob=O(chatId=chat_id(20),lastMessageId=msg_id(20,4),connectionProfileId=PROF)),
 O(op='enqueue',name='enqueue_again_dedupes',limit=None),
 O(op='enqueue',name='enqueue_capped',limit=1),
 O(op='handler',name='handler_covered',chatId=chat_id(20),foldOtherCatchup=True),
 O(op='handler',name='handler_truthy_yes',chatId=chat_id(28),foldOtherCatchup='yes'),
 O(op='handler',name='handler_ignore_idle_active',chatId=chat_id(29),foldOtherCatchup=True,foldOtherIgnoreIdle=True,plantLastMessageAtNow=True),
 O(op='handler',name='handler_not_idle',chatId=chat_id(23),foldOtherCatchup=True,plantLastMessageAtNow=True),
 O(op='handler',name='handler_missing_chat',chatId='c0ffffff-0000-4000-8000-000000000000',foldOtherCatchup=True),
 O(op='handler',name='handler_timeout',chatId=chat_id(30),foldOtherCatchup=True),
]
spec=O()
spec['_comment']=('Tier-3 corpus for the fold-grain OTHER pass (P4.D270, v4 `d58548051` lib/memory/fold-other-pass.ts + '
 'maintenance/fold-other-catchup.ts + the MEMORY_EXTRACTION handler branch, pin `01a83539d`). Consumed by '
 'build-fold-other-pass-fixture.ts (the seed), fold-other-pass-tier3.test.ts (v4 REAL code) and '
 'fold_other_pass_tier3_equivalence.rs. GENERATED by harness/oracle/scripts/gen-fold-other-pass-spec.py — edit the generator, not the JSON, '
 'when the ops move. Completion calls classify by the GRAIN block and are keyed by the `[op:<tag>]` the '
 'orienting chat summary carries and the OBSERVER label; embeddings are record-and-replay (one-hot per '
 'distinct text, in first-seen order).')
spec['_safety']='testPepperBase64 is the same THROWAWAY key the other fixtures use — never a real pepper.'
spec['testPepperBase64']='ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk='
spec['userId']=USER
spec['seedTimestamp']='2020-01-01T00:00:00.000Z'
spec['futureTimestamp']='2099-01-01T00:00:00.000Z'
spec['fixedNow']=FIXED_NOW
spec['connectionProfileId']=PROF
spec['chatSettings']=O(cheapLLMSettings=O(strategy='PROVIDER_CHEAPEST',fallbackToLocal=False),dangerMode='AUTO_ROUTE')
spec['connectionProfiles']=[O(id=PROF,name='Current Anthropic',provider='ANTHROPIC',modelName='claude-benchmark-large',isCheap=False,isDangerousCompatible=False),
                            O(id=CHEAP,name='Cheap OpenAI',provider='OPENAI',modelName='gpt-4o-mini',isCheap=True,isDangerousCompatible=False)]
chars=[]
for k,cid in C.items():
    c=O(id=cid,name=NAMES[k],controlledBy=('user' if k=='cass' else 'llm'),identity=f'{NAMES[k]} keeps the harbor books.',pronouns=O(subject=PRON[k][0],object=PRON[k][1],possessive=PRON[k][2]))
    c['files']=[]
    if k=='dov':
        c['files']=[O(path='Others/Ada.md',content='Ada is the harbormaster\'s most reliable clerk.'),
                    O(path='Commonplace/Ada.md',content='---\ntype: commonplace-digest\nsubjectCharacterId: "%s"\nupdatedAt: "2026-08-01T00:00:00.000Z"\n---\n# Ada\n\n- Ada counts every crate twice.\n' % C['ada'])]
    chars.append(c)
spec['characters']=chars
spec['ballast']=[O(characterId=C['eve'],count=4),O(characterId=C['fay'],count=2)]
spec['plants']=[O(name='qt_fop_gate_fail',content=f'[gate_fail] Ada noticed that Bram handled the harbor task 1.',characterId=C['ada'],message='planted gate write failure')]
spec['chats']=chats
spec['completionRules']=rules
spec['ops']=ops
import sys
open(sys.argv[1],'w').write(json.dumps(spec,indent=2,ensure_ascii=False)+'\n')
print(len(chats),'chats',len(rules),'rules',len(ops),'ops')
