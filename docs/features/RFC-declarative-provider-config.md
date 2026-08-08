# Feature Request: Declarative provider configs for image/LLM providers

**Status:** Proposed
**Target:** Post-5.0
**Author:** Charlie Sebold
**Depends on:** quilltap-v5 provider abstraction (Rust backend)

---

## Summary

v5 drops the TypeScript plugin system. In its place, ship a small number of
**built-in provider *shapes*** for the converged APIs, plus a **user-authored
declarative "provider spec"** (a config file, not code) that describes how to
talk to any other provider — especially the non-standard image APIs. This keeps
the common path zero-config while preserving flexibility for the weird cases,
without reintroducing arbitrary code execution.

The two halves:

1. **Built-in shapes** — bundled adapters for the API *shapes* that have
   converged, pointed at any compatible base URL.
2. **Provider specs** — declarative configs (TOML) that describe auth, request,
   control flow, and response extraction for everything else. Distributed via a
   curated Git registry with an "add from URL" escape hatch.

---

## Motivation

Text LLM APIs have largely converged. OpenAI Chat Completions is a de facto
standard: Groq, Together, Mistral, DeepSeek, Ollama, vLLM and others expose
Chat-Completions-compatible endpoints. So "Chat Completions + Responses +
Anthropic Messages" is not three *vendors* — it is three *shapes* that cover most
of the market. Framing the built-ins as shapes (each accepting an arbitrary base
URL) means a user can point the Chat Completions adapter at any compatible
endpoint with no new code.

Image APIs have **not** converged. They vary in request layout, auth placement,
sync-vs-async control flow, and output form (inline base64 vs URL vs nested
array). This is the long tail that used to justify plugins. A declarative spec
covers the great majority of it as *data*, which is safer to distribute and
review than code, and fits a plugin-free v5.

### Why config beats plugins here

- **Smaller security surface.** A spec describes HTTP; it cannot execute
  arbitrary code. That is the central win and the thing the design must protect.
- **Reviewable.** A TOML file in a PR is auditable at a glance.
- **Portable.** No build step, no Node runtime, no version-skew against the host.

The surface is not *zero* (see Security), but it is dramatically smaller than a
code plugin.

---

## Built-in shapes

Bundle three request/response shapes, each parameterized by base URL + model:

| Shape | Covers |
|---|---|
| OpenAI Chat Completions | OpenAI + all compatible endpoints (Groq, Together, Mistral, DeepSeek, Ollama, vLLM, …) |
| OpenAI Responses | OpenAI Responses API |
| Anthropic Messages | Anthropic |

Everything else is a provider spec (below). Note: **Google is a candidate to move
*out* of built-in support and be expressed as a provider spec instead** — see the
worked example. If the spec can express Google cleanly, that both simplifies the
built-in set and demonstrates the spec handles a real top-tier provider, not just
fringe cases.

---

## Provider spec model

A provider spec has **five** concerns. Response extraction alone is not enough —
the WaveSpeed reference case (below) proves you also need control flow and value
transforms.

1. **auth** — where the API key goes: named `bearer` header, custom header, or
   query param. Nothing else.
2. **request** — method, URL (with `{model}` and other interpolations), and a
   body template that may contain literals and interpolated/transformed fields.
3. **flow** — `sync` or `poll`. Poll carries a result URL, interval, timeout, and
   a done-condition.
4. **response** — a JMESPath extraction expression, output classification, and a
   post-fetch action for URL outputs.
5. **transforms** — a fixed set of *named, built-in* value transforms
   (`replace`, `fetch_and_inline`, `strip_data_url_prefix`, …). **No inline
   code, ever.** The named-transform set is the boundary between "config" and
   "plugin"; keep it small and additive.

Anything a spec cannot express (e.g. bespoke model-discovery logic with custom
filtering) is explicitly out of scope — that is the ~5% "fork it" tail. Do not
grow the DSL toward Turing-completeness; a config language that can express
everything is just a plugin system with worse ergonomics (the inner-platform
trap).

### Query language: JMESPath (not JSONPath)

**Decision: JMESPath.** Two reasons that matter given the constraints:

1. **One stable spec + shared compliance suite across languages.** Specs are
   user-authored, so an author tests an expression in some tool and it then runs
   in the Rust backend — those must agree exactly. JMESPath has had a single
   specification and a shared cross-language compliance test suite for years, so
   Rust / JS / Python behave identically. JSONPath was only standardized in
   RFC 9535 (2024); pre-standard dialects still circulate in tooling, so
   "works in the online evaluator, breaks in prod" is a real risk.
2. **It expresses the coalesce case directly.** JMESPath `||` or-expressions,
   pipes, and functions map straight onto the polymorphic-output problem:
   `outputs[0].url || outputs[0].data || outputs[0]`. Base JSONPath only
   *selects* — no coalesce, no functions — so that fallback logic would leak back
   into Rust, defeating the purpose of a declarative spec.

**Rust support:** the `jmespath` crate is mature and is the intended backend
dependency. (Pin/verify the current version at implementation time.)

**Honest counterpoint:** JSONPath is more widely *recognized* by casual authors,
and `serde_json_path` (RFC 9535) is a good crate today. But recognition does not
outweigh the need for extraction logic to actually live in the config rather than
in Rust. JMESPath wins on the merits for this use case.

**Note on scope:** JMESPath answers "*where* is the data." It does not do value
*transforms* (`"1024x1024"` → `"1024*1024"`, download-and-base64). Those are the
named transforms in concern (5) and are needed regardless of query language.

---

## Distribution / registry

**Recommended: a curated Git registry + an escape hatch.**

- **Curated registry** — a single repo (`quilltap-providers`) with
  `providers/*.toml`. Contributions arrive by PR and are reviewed before being
  "blessed." The app ships/pins a snapshot or fetches from raw Git URLs. Free,
  reviewable, versioned.
- **Escape hatch** — `add from URL` for power users who accept the risk of an
  unreviewed spec.
- **Maintainer-seeded examples** — you maintain a couple (WaveSpeed, Google) so
  contributors have a reference for what "good" looks like.

**Not recommended as the primary channel: open GitHub topic-tag discovery**
(e.g. searching for a `quilltap-provider` topic). It is decentralized but
unvetted, and every spec describes outbound HTTP with the user's key injected —
running arbitrary unreviewed specs is the exact risk the curated flow avoids.
Fine as an opt-in extension of the escape hatch, not the default.

**Format:** TOML — comments, human-friendly, idiomatic in Rust via serde —
validated against a JSON Schema. Include a `spec_version` field so the DSL can
evolve without breaking old specs. Version registry entries with semver, pinned
by the app.

---

## Security model

Config is not code, but the surface is not zero. Three guardrails:

1. **Egress allowlist.** Each spec declares the domains it may contact; the host
   enforces it (reuse the existing plugin manifest `permissions.network`
   pattern). A spec cannot exfiltrate to an arbitrary host.
2. **Constrained key placement.** The API key may only be placed in a named
   header or query slot. A spec cannot put the key in a request body field or an
   arbitrary location.
3. **Named transforms only.** No inline code / expressions with side effects.
   The transform set is a fixed, host-defined allowlist.

With these, the worst a malicious spec can do is talk to its own allowlisted
domain with the key the user already gave that provider — materially safer than a
code plugin.

---

## Worked example 1: WaveSpeed (the oddball)

WaveSpeed exercises nearly every hard case at once, which is why it is the
reference. From the existing plugin:

- **Async submit→poll→fetch.** The SDK's `client.run(...)` hides a submit-job /
  poll-status / retrieve-result cycle (180s timeout, 1s interval). → `flow = poll`.
- **Value transform on a field.** `"1024x1024"` → `"1024*1024"` (asterisk, not
  `x`). → named `replace` transform.
- **Polymorphic, sniffed output.** Each output is a string *or* an object; the
  payload is `url ?? data ?? b64_json`, then classified as data-URL / http-URL /
  raw-base64. → JMESPath coalesce + `classify = auto`.
- **URL outputs must be downloaded and re-encoded** to base64. → `fetch_and_inline`.
- **Static provider flags.** `enable_base64_output`, `output_format`, `seed = -1`.
  → literal body fields.
- **Model in the URL path**, not the body.

The model-listing endpoint (with its text-to-image filtering heuristics) is
genuine code and is intentionally **out of scope** — the "fork it" tail.

```toml
spec_version = "1"
name = "wavespeed"

[auth]
type = "bearer"                      # Authorization: Bearer {key}

[request]
method = "POST"
url = "https://api.wavespeed.ai/api/v3/{model}"

[request.body]
prompt = "{prompt}"
size = { from = "size", transform = "replace:x=>*" }
seed = -1
output_format = "png"
enable_base64_output = true

[flow]
mode = "poll"
timeout_s = 180
interval_s = 1
done_when = "status == 'completed'"

[response]
extract = "outputs[0].url || outputs[0].data || outputs[0]"
classify = "auto"                    # data-url | http-url | raw-base64
on_url = "fetch_and_inline"

[permissions]
network = ["api.wavespeed.ai"]
```

## Worked example 2: Google (the proof case)

Google is deliberately included to show the spec handles a *top-tier* provider,
not just fringe ones — the argument for pulling Google out of built-in support.
The shape: API key as query param, nested `instances` / `parameters` request,
base64 under `predictions[0].bytesBase64Encoded`, no post-transform needed
(already base64).

```toml
spec_version = "1"
name = "google-imagen"

[auth]
type = "query"
param = "key"                        # ...:predict?key={key}

[request]
method = "POST"
url = "https://generativelanguage.googleapis.com/v1beta/models/{model}:predict"

[request.body]
instances = [ { prompt = "{prompt}" } ]
parameters = { sampleCount = 1 }

[flow]
mode = "sync"

[response]
extract = "predictions[0].bytesBase64Encoded"
classify = "raw-base64"

[permissions]
network = ["generativelanguage.googleapis.com"]
```

> Verify Google's current request/response field names and endpoint against the
> live API docs before treating this example as authoritative.

---

## Open questions

1. **Spec versioning & migration** — how aggressively can the DSL evolve, and do
   old `spec_version` files get an automated upgrade path?
2. **Transform set scope** — what is the minimal but sufficient set of named
   transforms? (`replace`, `fetch_and_inline`, `strip_data_url_prefix` are the
   known-needed ones; resist growth.)
3. **Poll done-condition expressiveness** — is a single JMESPath boolean enough,
   or are error/failed states needed too (e.g. `status == 'failed'` → surface
   provider error)?
4. **Registry trust tiers** — do "blessed" (reviewed) vs "from URL" (unreviewed)
   specs get different UI treatment / warnings?
5. **Multi-image requests** — WaveSpeed caps at 1/request; does the extract
   expression need to yield a list for providers that return several?

---

## Non-goals

- Reintroducing code plugins.
- A Turing-complete config DSL.
- Expressing bespoke model-discovery/filtering logic in specs.
