/**
 * P4.D305 (v4 `362426c8f`) — plant trigger-bearing `.tool.json` files into
 * characters' vaults for a tier-3 fixture builder, through v4's REAL
 * `storeMountFile` (never a hand-written row). Shared by
 * `build-orchestrator-fixture.ts` and `build-message-finalizer-fixture.ts`.
 *
 * Run inside a builder, after the characters exist (cwd = the v4 checkout).
 */

export interface TriggerToolSpec {
  characterId: string;
  /** The character's `metadata.json` fact sheet; absent = leave it as provisioned. */
  metadata?: Record<string, unknown>;
  tool: { name: string };
}

// v4's repository container (`getRepositories()`).
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function writeTriggerTools(repos: any, specs: TriggerToolSpec[] | undefined): Promise<void> {
  if (!specs || specs.length === 0) return;
  const { storeMountFile } = await import('@/lib/mount-index/store-file');
  const writeVaultFile = async (mountPointId: string, relativePath: string, body: unknown) => {
    await storeMountFile({
      mountPointId,
      relativePath,
      data: Buffer.from(JSON.stringify(body, null, 2), 'utf8'),
      originalMimeType: 'application/json',
      transcodeImages: false,
      extractText: false,
      enqueueEmbedding: false,
      force: true,
    } as never);
  };
  for (const t of specs) {
    const raw = (await repos.characters.findByIdRaw(t.characterId)) as {
      characterDocumentMountPointId?: string | null;
    } | null;
    const vault = raw?.characterDocumentMountPointId;
    if (!vault) throw new Error(`trigger tool: character ${t.characterId} has no vault`);
    if (t.metadata) await writeVaultFile(vault, 'metadata.json', t.metadata);
    await writeVaultFile(vault, `Tools/${t.tool.name}.tool.json`, t.tool);
  }
}
