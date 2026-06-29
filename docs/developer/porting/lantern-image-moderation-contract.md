# Native Core: The Lantern Refusal-Handling Contracts

> Status: **agreed design**, to be implemented in Phase 3 when the Lantern
> (image subsystem) is ported. Companion to
> [`provider-manifest.md`](./provider-manifest.md) — that note's "Surprise 2"
> (Google's empty-200 safety rejection) is the thread the first half pulls on.
> Records **two** cross-component invariants a naïve port silently drops, both
> keyed off the Concierge's uncensored configuration but otherwise independent:
> (1) the **post-hoc image-moderation reroute** (provider rejects an issued
> image → retry on uncensored *image* profile), carried by an error-message
> string rather than a type; and (2) the **pre-hoc LLM-refusal retry** (a safe
> cheap-LLM refuses appearance/prompt work → retry on uncensored *LLM* profile),
> carried by an `llmResolved`/empty-success signal. They are separate porting
> units with separate tests. Decided with Charlie, 2026-06-29.

## The one-sentence problem

In v4, an image provider signals "the model refused this prompt on content
grounds" by **throwing an `Error` whose message contains certain keywords**, and
a completely separate component (the Concierge / story-background handler)
pattern-matches that string to decide whether to **retry the image on the user's
configured uncensored profile**. The signal is stringly-typed and the contract
is implicit. Port it as-is and it works until someone rewords an error; port it
carelessly and the uncensored-fallback path silently dies with no test catching
it.

## The contract as it exists in v4 (the oracle)

Three pieces, in three files:

**1. Providers must produce a recognizable rejection.** Each image provider, on
a content refusal, throws an error whose message matches one of these
substrings (case-insensitive), per `isImageModerationError` in
`lib/services/dangerous-content/provider-routing.service.ts`:

```
"content moderation"   "content_policy"   "content policy"
"safety system"        "rejected by content"   "moderation_blocked"
```

The providers reach that shape differently — and this is the part the manifest
port must preserve per provider:

- **OpenAI / DALL·E** — the SDK throws with *"Your request was rejected as a
  result of our safety system."* → matches `safety system`.
- **Grok** — throws *"Generated image rejected by content moderation."* →
  matches `content moderation` / `rejected by content`.
- **Google Imagen (`:predict`)** — **does not throw.** It returns **HTTP 200**
  with an empty `predictions` array (or predictions carrying
  `raiFilteredReason` and no image bytes). The v4 plugin **manufactures** the
  error: it throws `"Google Imagen rejected prompt by content policy: <reason>"`
  → matches `content policy`. This manufacturing step is load-bearing; without
  it the empty 200 looks like a generic "no images" failure and the reroute
  never fires.
- **OpenRouter** — image-gen rides the chat endpoint; a refusal arrives as
  `message.refusal` or as text content, and the plugin throws
  *"Model declined to generate an image: …"*. **Note:** this phrasing does
  **not** currently match `isImageModerationError`'s keyword set — see "Gaps
  worth fixing in the port" below.

**2. The reroute resolver decides if a second attempt is allowed.**
`resolveUncensoredImageProfileForReroute(currentProfileId, dangerSettings, userId)`
returns a profile+key or `null`. It returns `null` (caller surfaces the original
error) when **any** of:

- `dangerSettings.mode !== 'AUTO_ROUTE'`
- no `uncensoredImageProfileId` configured
- the configured uncensored profile **is** the one that just rejected (loop
  guard)
- the profile can't be loaded, is owned by another user, or has no usable key

It deliberately does **not** scan for any `isDangerousCompatible` profile — the
post-hoc reroute is keyed strictly on the user's explicit uncensored choice, so
a silent scan can't surface a profile the user didn't pick. Carry that
restriction forward verbatim; it's a privacy/consent decision, not an oversight.

**3. The handler wires them together** (`background-jobs/handlers/story-background.ts`):

```
try { generate image on imageProfile }
catch (error) {
  reroute = isImageModerationError(error)
    ? await resolveUncensoredImageProfileForReroute(imageProfile.id, dangerSettings, userId)
    : null
  if (!reroute) throw new Error(`Image generation failed: ${errorMessage}`)
  // else: regenerate on reroute.profile, set activeImageProfile = reroute.profile
}
```

`activeImageProfile` is then used for the saved file's `generationModel`
metadata — so the reroute also has to be observable in the output, not just in
logs. The same `isImageModerationError` helper is reused by the character-avatar
and inline `generate_image` handlers (the comment in story-background says so
explicitly), so it is a **shared** contract, not a one-off.

There is also a *pre-hoc* sibling path — a separate porting unit, documented in
full in the next section.

## The pre-hoc LLM-refusal retry path (a distinct porting unit)

Before any image bytes are requested, the story-background handler runs **two
cheap-LLM steps** that can themselves be refused by a safety-aligned model, and
each has its **own** retry-on-uncensored-LLM-profile logic. This is mechanically
distinct from the post-hoc image reroute above: the signal is a **text-side LLM
refusal**, detected structurally (not by `isImageModerationError`), and the
retry swaps the **LLM** selection (`uncensoredLLMSelection`), not the image
profile. Port it as its own unit — it's easy to skip precisely because it
doesn't go through the image-moderation helper.

**The uncensored-LLM selection is built once** near the top of the handler from
`chatSettings.cheapLLMSettings.imagePromptProfileId` (resolved to a
`CheapLLMSelection`, with the Ollama-localhost baseUrl special-case). If the chat
is *already* marked dangerous, the handler skips the safe provider entirely for
appearance work (`appearanceLLMSelection = uncensoredLLMSelection`) on the theory
the safe one will just refuse. The retries below only fire when the **safe**
provider was the one used.

**Step 1 — appearance resolution** (`resolveCharacterAppearances` →
`AppearanceResolutionResult { appearances, llmResolved }`):

- The refusal signal is **`llmResolved === false`**. Crucially this flag
  conflates three cases the resolver treats identically: the cheap LLM errored,
  it refused, or it returned `success` with an empty/zero-length result
  (`result.success && (!result.result || length === 0)` → `llmResolved: false`).
  An **empty success is deliberately read as a silent content refusal.**
- It is **not** raised for the legitimate no-LLM shortcuts: scene-state
  appearances supplied, or `canSkipResolution(...)` true (trivial context) — both
  return `llmResolved: true` with defaults. The retry must distinguish "refused"
  from "didn't need the LLM"; only the former (`!llmResolved` **and**
  `appearanceInputs.length > 0` **and** the safe provider was used **and** an
  uncensored selection exists) triggers a retry.
- Retry: re-run `resolveCharacterAppearances` with `uncensoredLLMSelection`. If
  the retry's `llmResolved` is true, adopt its appearances; otherwise keep the
  defaults and continue (this step degrades gracefully — it does not abort the
  job).

**Step 2 — prompt crafting** (`craftStoryBackgroundPrompt` →
`{ success, result, error }`):

- Two distinct failure shapes, handled differently:
  - **`!success`** → a real cheap-LLM error → log and **return (abort the job)**.
    No retry on this branch.
  - **`success` but empty `result`** → **treated as a silent content refusal** →
    *this* is the branch that retries on `uncensoredLLMSelection`.
- Retry: re-run `craftStoryBackgroundPrompt` with `uncensoredLLMSelection`. On
  retry success adopt `finalPrompt`; on retry failure **abort the job**
  (`return`). If no uncensored profile is configured, abort with a "cannot retry"
  log.

**Why this is its own unit, and its porting obligations:**

- It shares *nothing* with `isImageModerationError` — do not try to unify them.
  The text-refusal signal is `llmResolved === false` / empty-success, surfaced by
  the cheap-LLM call shape, and belongs with the **cheap-LLM task** port (the
  same family as the memory/summarization cheap tasks already in Phase 1), not
  the image-decoder port.
- The **empty-success-as-refusal** heuristic appears in *both* steps and is the
  subtle invariant a port silently drops (a port that treats empty-success as
  generic success would never retry, and dangerous-chat image-gen would quietly
  stop working for refused prompts). Carry it verbatim and test it explicitly.
- The two steps have **different failure dispositions** — appearance resolution
  degrades to defaults and continues; prompt crafting aborts the job. Don't
  flatten them to one policy.
- Differential obligations: a corpus exercising, per step, (a) safe-LLM success
  → no retry, (b) safe-LLM hard error, (c) safe-LLM empty-success → retry fires,
  (d) retry success, (e) retry failure / no uncensored profile → correct
  disposition (continue-with-defaults vs abort). Plus the
  already-dangerous-chat short-circuit that skips the safe provider for
  appearances.

This note is scoped to the Lantern's two refusal-handling families (post-hoc
image reroute + pre-hoc LLM retry). Both are Phase-3, both keyed off the
Concierge's uncensored configuration, but they are independent ports and should
land as separate units with separate tests.

## How to port it to v5 (the recommendation)

Replace the stringly-typed signal with a **typed rejection that crosses the
provider boundary**, while keeping a string-matching *compatibility shim* for the
providers whose SDKs only give us a message.

1. **A typed error in the image-provider boundary.** The image decoders (see
   `provider-manifest.md`, the `*-images` enum) return
   `Result<ImageGenResponse, ImageGenError>`, where `ImageGenError` has a
   variant:

   ```rust
   enum ImageGenError {
       Moderation { provider: ProviderId, reason: Option<String>, raw: String },
       // … transport, auth, invalid-response, etc.
   }
   ```

   Each decoder is responsible for **mapping its provider's refusal shape into
   `Moderation`** — including Google's manufactured case (empty `predictions` /
   `raiFilteredReason` → `Moderation`) and OpenAI/Grok's SDK message. This moves
   the "manufacture the error" logic from a plugin into the decoder where it
   belongs, and makes it the decoder's tier-1 differential test obligation.

2. **A keyword shim for message-only providers.** Keep the
   `isImageModerationError` keyword set as a fallback classifier for any path
   that only yields a message string (e.g. an OpenAI-SDK error we don't
   structurally inspect). Port the exact substring list so behavior matches the
   oracle; treat it as a *recognizer of last resort*, not the primary signal.

3. **The reroute resolver becomes a pure-ish decision function.** Port
   `resolveUncensoredImageProfileForReroute` with all four `null` guards intact
   (mode, configured id, loop guard, load/ownership/key). Its only impurity is
   the profile+key lookup; structure it so the decision logic is unit-testable
   with the lookups injected (matches the differential discipline — fake repos,
   recording the decision).

4. **Preserve the observable output.** The reroute must update the equivalent of
   `activeImageProfile` so the persisted image's `generationModel` reflects the
   profile that actually produced it. A tier-2 DB-state test on the saved image
   row is the natural guard.

## Differential test obligations

- **Per image decoder (tier-1):** feed a recorded moderation-rejection response
  (the real empty-200 from Imagen, the real DALL·E safety-system error, the
  Grok phrasing, the OpenRouter refusal) and assert it maps to
  `ImageGenError::Moderation` — and that a *success* response does not.
- **`isImageModerationError` shim (tier-1 exact):** the ported keyword
  recognizer must classify a fixed corpus identically to v4's function,
  including the negatives.
- **Reroute resolver (tier-2 / decision trace):** each of the four `null`
  branches plus the success branch, with the loop-guard case
  (`uncensoredImageProfileId === currentProfileId`) explicitly covered — it's
  the easiest one to drop.

## Gaps worth fixing in the port (flagged, not silently "fixed")

These are v4 behaviors that look like latent bugs. The port should **match v4
first** (oracle discipline), then fix deliberately with the change recorded:

- **OpenRouter refusals don't match the keyword set.** OpenRouter throws *"Model
  declined to generate an image: …"*, which contains none of the six keywords,
  so an OpenRouter content refusal **never triggers the uncensored reroute**
  today. If OpenRouter is a supported image route, this is a real hole — but
  closing it changes behavior, so do it as a named follow-up with its own test,
  not as a quiet port-time "improvement."
- **Keyword brittleness generally.** Any provider rewording its safety message
  silently breaks the reroute. The typed-`Moderation` approach above is the
  structural fix; the keyword shim should shrink to covering only message-only
  paths over time.

## Provenance

Grounded in v4 source read directly:
`lib/services/dangerous-content/provider-routing.service.ts`
(`isImageModerationError`, `resolveUncensoredImageProfileForReroute`),
`lib/background-jobs/handlers/story-background.ts` (the try/catch reroute wiring
and `activeImageProfile`), and the five image-provider plugins inventoried in
[`provider-manifest.md`](./provider-manifest.md).
