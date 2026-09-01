import { describe, expect, it } from 'vitest';

import { extractHuggingFaceRepoId, huggingFaceCardUrl } from './huggingface-repo-id';
import vectors from './__fixtures__/huggingface-repo-id-vectors.json';

/**
 * Parity spec for the LoRA-source repo-id twin. The oracle is v4's PURE module
 * `lib/image-gen/huggingface-repo-id.ts` at `2ece98c90`.
 *
 * Two halves. v4's own table lives not beside the module but inside
 * `__tests__/unit/image-gen/huggingface-lookup.test.ts` (the module is
 * re-exported from the lookup), and is transcribed 1:1 below. The recording
 * that follows it is taken from v4's REAL functions run at the pin (recipe:
 * `harness/oracle/cases/huggingface-repo-id.test.ts`), and reaches what those
 * three cases cannot: every interesting edge here is a question about
 * `new URL(...)` and the hostname regex rather than about the source string,
 * which is exactly the kind a hand-written table invents answers for.
 */
describe('huggingface-repo-id (v4 unit table 1:1)', () => {
  it('accepts a bare owner/name', () => {
    expect(extractHuggingFaceRepoId('XLabs-AI/flux-RealismLora')).toBe(
      'XLabs-AI/flux-RealismLora',
    );
    expect(extractHuggingFaceRepoId('  Datou1111/shou_xin  ')).toBe('Datou1111/shou_xin');
    expect(extractHuggingFaceRepoId('ostris/flux2_berthe_morisot')).toBe(
      'ostris/flux2_berthe_morisot',
    );
  });

  it('recovers the repository from a huggingface.co weights URL', () => {
    expect(
      extractHuggingFaceRepoId(
        'https://huggingface.co/lovis93/Flux-2-Multi-Angles-LoRA-v2/resolve/main/weights-fal.safetensors',
      ),
    ).toBe('lovis93/Flux-2-Multi-Angles-LoRA-v2');
    expect(extractHuggingFaceRepoId('https://huggingface.co/owner/name')).toBe('owner/name');
  });

  it('declines anything with no repository behind it', () => {
    // Weights hosted elsewhere have no card to read, so the editor must not
    // offer a button that could only ever fail.
    expect(extractHuggingFaceRepoId('https://cdn.example.com/weights.safetensors')).toBeNull();
    expect(extractHuggingFaceRepoId('')).toBeNull();
    expect(extractHuggingFaceRepoId('   ')).toBeNull();
    expect(extractHuggingFaceRepoId('justonesegment')).toBeNull();
    expect(extractHuggingFaceRepoId('too/many/segments')).toBeNull();
    expect(extractHuggingFaceRepoId('owner name/with space')).toBeNull();
    expect(extractHuggingFaceRepoId('https://huggingface.co/owner')).toBeNull();
    // A lookalike host must not be mistaken for the registry.
    expect(extractHuggingFaceRepoId('https://nothuggingface.co/owner/name')).toBeNull();
  });

  it('points at the public model card', () => {
    expect(huggingFaceCardUrl('owner/name')).toBe('https://huggingface.co/owner/name');
  });
});

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
