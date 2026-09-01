import { describe, expect, it } from 'vitest';

import { extractHuggingFaceRepoId, huggingFaceCardUrl } from './huggingface-repo-id';
import vectors from './__fixtures__/huggingface-repo-id-vectors.json';

/**
 * Parity spec for the LoRA-source repo-id twin. The oracle is v4's PURE module
 * `lib/image-gen/huggingface-repo-id.ts` at `2ece98c90`.
 *
 * v4 ships no unit test for this module, so there is nothing to transcribe:
 * the whole differential is the recording below, taken from v4's REAL
 * functions run at the pin (recipe:
 * `harness/oracle/cases/huggingface-repo-id.test.ts`). Every interesting edge
 * is a question about `new URL(...)` and the hostname regex rather than about
 * the source string, which is exactly the kind a hand-written table invents
 * answers for.
 */
describe('huggingface-repo-id vs v4’s recorded output (2ece98c90)', () => {
  for (const v of vectors) {
    it(`extractHuggingFaceRepoId(${JSON.stringify(v.source)})`, () => {
      const repoId = extractHuggingFaceRepoId(v.source);
      expect(repoId).toBe(v.repoId);
      expect(repoId === null ? null : huggingFaceCardUrl(repoId)).toBe(v.cardUrl);
    });
  }

  it('the corpus discriminates the hostname regex’s three arms', () => {
    // `(^|\.)huggingface\.co$` accepts the bare host and any subdomain, and
    // refuses a host that merely ENDS with the string or merely starts with
    // it. A `includes('huggingface.co')` twin passes nothing here.
    const row = (source: string) => vectors.find((v) => v.source === source);
    expect(row('https://huggingface.co/owner/name')?.repoId).toBe('owner/name');
    expect(row('https://hf.huggingface.co/owner/name')?.repoId).toBe('owner/name');
    expect(row('https://nothuggingface.co/owner/name')?.repoId).toBeNull();
    expect(row('https://huggingface.co.evil.example/owner/name')?.repoId).toBeNull();
    // The `$` anchor also refuses a fully-qualified trailing dot — `URL`
    // keeps it in `hostname`, so this is a real (if obscure) refusal.
    expect(row('https://huggingface.co./owner/name')?.repoId).toBeNull();
  });

  it('the corpus pins the FIRST-TWO-SEGMENTS rule, quirk and all', () => {
    const row = (source: string) => vectors.find((v) => v.source === source);
    // The deep-link form the feature exists for: the reason it is first-two
    // rather than last-two.
    expect(
      row('https://huggingface.co/owner/model-name/resolve/main/weights.safetensors')?.repoId,
    ).toBe('owner/model-name');
    // And the quirk that follows from it: a `/models/` prefix — which the site
    // does serve — yields `models/owner`, not `owner/name`. v4 does this; v5
    // reproduces it rather than "fixing" it.
    expect(row('https://huggingface.co/models/owner/name')?.repoId).toBe('models/owner');
  });

  it('the corpus discriminates REPO_ID_PATTERN’s leading-character class', () => {
    const row = (source: string) => vectors.find((v) => v.source === source);
    // Each segment must OPEN alphanumeric even though `.`/`-`/`_` are legal
    // inside it. A twin using a single `[A-Za-z0-9._-]+` class per segment
    // passes every happy case and fails these three.
    expect(row('owner/.hidden')?.repoId).toBeNull();
    expect(row('.owner/name')?.repoId).toBeNull();
    expect(row('owner/-name')?.repoId).toBeNull();
    expect(row('owner/name.safetensors')?.repoId).toBe('owner/name.safetensors');
  });

  it('the corpus pins what is NOT treated as a URL at all', () => {
    const row = (source: string) => vectors.find((v) => v.source === source);
    // The `^https?://` test gates the URL branch; anything else falls to the
    // bare-repo-id branch, where slashes and colons lose.
    expect(row('huggingface.co/owner/name')?.repoId).toBeNull();
    expect(row('ftp://huggingface.co/owner/name')?.repoId).toBeNull();
    expect(row('//huggingface.co/owner/name')?.repoId).toBeNull();
    // ...but the scheme test is case-insensitive.
    expect(row('HTTPS://huggingface.co/owner/name')?.repoId).toBe('owner/name');
  });

  it('the corpus is non-trivial — it contains both outcomes in quantity', () => {
    // A guard on the corpus itself: a regeneration that silently collapsed to
    // all-null (the shape a broken import produces) cannot pass as coverage.
    const hits = vectors.filter((v) => v.repoId !== null);
    expect(hits.length).toBeGreaterThanOrEqual(15);
    expect(vectors.length - hits.length).toBeGreaterThanOrEqual(15);
  });
});
