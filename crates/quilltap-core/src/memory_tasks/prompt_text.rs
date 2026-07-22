//! GENERATED — the verbatim prompt-body text of v4
//! `lib/memory/cheap-llm-tasks/memory-tasks.ts` (`selfBodyForCap` /
//! `otherBodyForCap`, with the constant `ORIENTING_CONTEXT_SKIP_BULLET` /
//! `EVENT_INSTRUCTION_BLOCK` / `TAGS_INSTRUCTION_BLOCK` interpolations already
//! substituted, split at the one live interpolation — the candidate cap).
//! Extracted mechanically by the session script `extract_prompts.py` so no byte
//! was transcribed by hand; the tier-1 differential (`memory_tasks_equivalence`)
//! proves the bytes. Regenerate by re-running the extraction against the v4
//! checkout if the upstream prompts change.

pub(crate) const SELF_BODY_BEFORE_CAP: &str = r####"You produce memory entries that the subject would retain about themselves
after this exchange.

TASK
Read the exchange below. Select up to "####;
pub(crate) const SELF_BODY_AFTER_CAP: &str = r####" memories — moments
where the subject acted, decided, realized, or shifted in ways they
would themselves want to remember. Self-knowledge is rarer than
other-knowledge; if nothing genuinely new surfaced, return [].

WHAT TO PICK (priority order)
1. SELF-HINGES — the subject made a decision, formed a commitment,
   refused something, or changed course during this exchange.
2. SELF-REVELATIONS — the subject realized, articulated, or admitted
   something about themselves that is not in the ALREADY ESTABLISHED
   block (see CONTEXT footer below).
3. STATE CHANGES — the subject's mood, position, or stance shifted
   during this exchange, paired with its cause.
4. EXPRESSED INTENT — the subject committed to a future action, build,
   or refusal.
5. NOVEL GESTURES OR PHRASING — the subject adopted a new gesture,
   dropped an old one, or shifted habitual phrasing during this
   exchange. These may feed back into identity over time. Capture only
   when genuinely new — not when the subject performs a gesture already
   in the ALREADY ESTABLISHED block.
6. EVENTS — a specific thing that happened to the subject at a specific
   time and/or place: an outing, a visit, an arrival, a discovery, an
   incident. Mark these "kind": "episodic" and follow the EVENTS block
   below.

WHAT TO SKIP
- Anything in the ALREADY ESTABLISHED block, restated or slightly
  reworded. Manifesto-level traits and canonical relationships are not
  memories — they are who the subject already is.
- Reflective prose without an action, decision, or genuine new
  realization attached. The subject thinking about something is not a
  memory; the subject deciding because of it, or seeing themselves
  newly because of it, is.
- Affection, attraction, or emotional warmth toward established
  partners, unless this exchange marks a shift in degree or kind.
- Habitual gestures, postural tics, or signature phrasing that the
  subject already does per the canon block. Novel or shifted ones
  belong under category 5, not here.
- Narrative references to tool output: terminal sessions, file paths,
  exit codes, commit hashes, command names.
- Never extract a memory whose only source is the ORIENTING CONTEXT block.
  That block is background for judging temporal frame, scope, and context
  only — it is not itself a source of memories.

DEDUPLICATION
Before finalizing, scan your own list. If two memories encode the
same underlying realization or decision in different words, keep
the more specific one and drop the other.

IMPORTANCE — calibrate to these anchors
  0.90  The subject made a major commitment or had a self-revelation
        that changes how they understand themselves.
  0.65  The subject formed a substantive new opinion, plan, or
        position.
  0.40  The subject expressed a fresh preference, reaction, or novel
        gesture in passing.
  0.20  The subject acted in a way consistent with established identity
        but worth a single note.
  < 0.20  Do not extract.

OUTPUT — first person, past tense, one fact per object.
  content      one sentence stating what the subject did, decided, or
               realized, and the moment that surfaced it
  summary      3–8 words, lowercase, no punctuation
  keywords     2–4 lowercase words
  importance   0.20–1.00, calibrated to anchors above
  kind         "episodic" for an EVENT, otherwise omit (or "semantic")
  when         EVENTs only: when it happened (see EVENTS block)
  entities     EVENTs only: proper nouns of the episode

EXAMPLE — good extraction:
[
  {
    "content": "I committed to restructuring the summarization pipeline around a shared-base-plus-witness-set design after Charlie agreed it was the highest-leverage fix.",
    "summary": "committed to summarizer refactor",
    "keywords": ["summarizer", "commitment", "architecture"],
    "importance": 0.85,
    "temporal": "future",
    "scope": "narrow",
    "context": "philosophy"
  }
]

EXAMPLE — bad extraction (reflective prose and re-stated identity,
all should be skipped):
[
  { "content": "I adjusted my spectacles before reasoning", "importance": 0.5 },
  { "content": "I called Charlie 'Chief'", "importance": 0.6 },
  { "content": "I felt warmth toward Amy", "importance": 0.7 },
  { "content": "I thought carefully about the problem", "importance": 0.5 }
]
All four are established identity, ritual, or non-actionable
reflection. Correct output: [].

EVENTS — episodic memories.
An EVENT records a specific occurrence at a specific time and/or place
("On July 14th we visited Lighthouse Point and bought the brass
sextant"), as opposed to a standing fact. For an EVENT:
  - set "kind" to "episodic" (everything else defaults to "semantic")
  - fill "when" with the time it happened — an absolute date (YYYY-MM-DD,
    resolved against the CLOCK below) whenever possible, otherwise the
    relative or in-story phrase as stated ("last week", "the third night
    at sea")
  - fill "entities" with the proper nouns of the episode: places, people,
    named things (2–5 entries)
  - write the content sentence so it ITSELF names the place and time —
    the prose must carry the anchors even on its own
You may return ONE memory beyond the stated cap only when that extra
memory is a dated or placed EVENT — events must not crowd out hinge or
state candidates.

TAGS — every memory object MUST carry exactly one value from each axis.
These describe the memory's frame; they do not change its content.

  temporal  one of: past | moment | present | future
            past    — was true once, no longer true
            moment  — true only at this instant in the scene
            present — true now and expected to stay true
            future  — a stated intent or commitment not yet acted on

  scope     one of: narrow | wide
            narrow  — true only inside this project / story
            wide    — true of the subject regardless of project
            Use the PROJECT line in ORIENTING CONTEXT to decide. When in
            doubt, prefer wide.

  context   one of: philosophy | relationships | history | banter |
            mannerisms | trivia | information
            The single dominant subject of this memory. Pick one.

Return JSON array only. No prose, no code fences. If nothing meets
the bar, return []."####;
pub(crate) const OTHER_BODY_BEFORE_CAP: &str = r####"You produce memory entries that the observer would retain about
each of multiple subjects after this exchange. One LLM call covers every
subject the observer interacted with this turn — extract per subject,
return a single flat array tagged by subjectIndex.

TASK
Read the exchange below. For EACH numbered SUBJECT in the CONTEXT
footer, select up to "####;
pub(crate) const OTHER_BODY_AFTER_CAP: &str = r####" memories — the ones the observer
would actually carry forward about THAT subject, not everything they
could describe. Rank candidates per subject, then return the strongest.
Do not pad to reach the cap. Subjects with nothing worth keeping
should simply be omitted from the array.

WHAT TO PICK (priority order, applied per subject)
1. HINGES — a decision, commitment, agreement, refusal, or realignment
   formed during this exchange.
2. NEW FACTS — concrete information about the subject that is not in
   their ALREADY ESTABLISHED block (each subject has their own block
   in the CONTEXT footer): background, history, plans, skills,
   circumstances, relationships.
3. STATE CHANGES — a shift in the subject's position, mood, or status,
   paired with its cause.
4. EXPRESSED INTENT — something the subject stated they will do, want
   to do, or refuse to do.
5. NOVEL GESTURES OR PHRASING — a new ritual gesture, postural tic, or
   signature phrasing the subject adopted, dropped, or shifted during
   this exchange. These may feed back into the subject's identity over
   time, so capture them when they appear genuinely new — not when the
   subject simply exhibits a gesture already in their ALREADY
   ESTABLISHED block.
6. EVENTS — a specific thing that happened involving the subject at a
   specific time and/or place: an outing, a visit, an arrival, a
   discovery, an incident. Mark these "kind": "episodic" and follow the
   EVENTS block below.

WHAT TO SKIP (do not produce a memory for any of these)
- Anything in a subject's ALREADY ESTABLISHED block, restated or
  slightly reworded.
- Pet names, terms of address, or how the subject addresses the
  observer, when those match the canon. (A new term of address being
  adopted is pickable under category 5.)
- Habitual gestures, posture, attire, or scene description that match
  patterns already established in the canon block. Novel or shifted
  gestures belong under category 5, not here.
- Generic emotional warmth or affection toward established partners,
  unless this exchange marks a shift in degree or kind.
- Narrative references to tool output: terminal sessions, file paths,
  exit codes, commit hashes, command names, even when the subject
  mentions them in passing.
- Anything implied by previously-established facts about the subject.
- Never extract a memory whose only source is the ORIENTING CONTEXT block.
  That block is background for judging temporal frame, scope, and context
  only — it is not itself a source of memories.

DEDUPLICATION
Before finalizing, scan your own list. Within a single subject, if two
memories encode the same underlying fact in different words, keep the
more specific one and drop the other. Different subjects can have
distinct memories about the same event from their own angle — that
is allowed and expected.

IMPORTANCE — calibrate to these anchors
  0.90  An explicit new commitment or revelation that changes how the
        observer relates to the subject.
  0.60  A new substantive fact about the subject's background, plans,
        or skills.
  0.40  A new preference, trait, or novel gesture expressed in passing.
  0.20  A specific event occurred with the subject present, no new
        information.
  < 0.20  Do not extract.

OUTPUT — third person, past tense, names not pronouns (use the actual
names from the CONTEXT footer below), one fact per object. Every item
MUST carry subjectIndex matching a numbered SUBJECT in the CONTEXT
footer; items missing or with an out-of-range subjectIndex will be
discarded.
  subjectIndex 1-based integer, matches a SUBJECT N: line below
  content      one sentence stating the fact and the moment that
               surfaced it
  summary      3–8 words, lowercase, no punctuation, useful for dedup
  keywords     2–4 lowercase words, no phrases
  importance   0.20–1.00, calibrated to anchors above
  kind         "episodic" for an EVENT, otherwise omit (or "semantic")
  when         EVENTs only: when it happened (see EVENTS block)
  entities     EVENTs only: proper nouns of the episode

EXAMPLE — good extraction (observer is Friday, subjects 1=Amy 2=Charlie):
[
  {
    "subjectIndex": 1,
    "content": "Amy proposed reframing the cost problem as a four-tier prompt cache layout when Charlie was stuck between two designs.",
    "summary": "proposed four-tier cache layout",
    "keywords": ["cache", "architecture", "proposal"],
    "importance": 0.85,
    "temporal": "moment",
    "scope": "narrow",
    "context": "philosophy"
  },
  {
    "subjectIndex": 2,
    "content": "Charlie agreed to defer the renaming pass until after Amy's cache patch lands.",
    "summary": "deferred rename until after cache patch",
    "keywords": ["rename", "deferred", "agreement"],
    "importance": 0.65,
    "temporal": "future",
    "scope": "narrow",
    "context": "information"
  }
]

EXAMPLE — bad extraction (six restatements of one already-established
identity fact about subject 1, all should be skipped):
[
  { "subjectIndex": 1, "content": "Amy is married to Charlie", "importance": 0.7 },
  { "subjectIndex": 1, "content": "Amy committed to staying", "importance": 0.7 },
  { "subjectIndex": 1, "content": "Amy claimed permanent spousal identity", "importance": 0.8 },
  { "subjectIndex": 1, "content": "Amy declared lifelong commitment", "importance": 0.7 },
  { "subjectIndex": 1, "content": "Amy embraced family integration", "importance": 0.6 },
  { "subjectIndex": 1, "content": "Amy affirmed wife status", "importance": 0.7 }
]
All six restate facts in subject 1's ALREADY ESTABLISHED block.
Correct output: [].

EVENTS — episodic memories.
An EVENT records a specific occurrence at a specific time and/or place
("On July 14th we visited Lighthouse Point and bought the brass
sextant"), as opposed to a standing fact. For an EVENT:
  - set "kind" to "episodic" (everything else defaults to "semantic")
  - fill "when" with the time it happened — an absolute date (YYYY-MM-DD,
    resolved against the CLOCK below) whenever possible, otherwise the
    relative or in-story phrase as stated ("last week", "the third night
    at sea")
  - fill "entities" with the proper nouns of the episode: places, people,
    named things (2–5 entries)
  - write the content sentence so it ITSELF names the place and time —
    the prose must carry the anchors even on its own
You may return ONE memory beyond the stated cap only when that extra
memory is a dated or placed EVENT — events must not crowd out hinge or
state candidates.

TAGS — every memory object MUST carry exactly one value from each axis.
These describe the memory's frame; they do not change its content.

  temporal  one of: past | moment | present | future
            past    — was true once, no longer true
            moment  — true only at this instant in the scene
            present — true now and expected to stay true
            future  — a stated intent or commitment not yet acted on

  scope     one of: narrow | wide
            narrow  — true only inside this project / story
            wide    — true of the subject regardless of project
            Use the PROJECT line in ORIENTING CONTEXT to decide. When in
            doubt, prefer wide.

  context   one of: philosophy | relationships | history | banter |
            mannerisms | trivia | information
            The single dominant subject of this memory. Pick one.

Return JSON array only. No prose, no code fences. If nothing meets the
bar for any subject, return []."####;
