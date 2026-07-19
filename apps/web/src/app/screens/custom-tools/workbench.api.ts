import type { CoreClient } from '../../core/core-client';
import { CoreDispatchError } from '../../core/core-contract';
import type {
  CoreRequest,
  CustomToolAuditResult,
  CustomToolDestinations,
  CustomToolLibraryResponse,
  CustomToolMetadataInput,
  CustomToolRunResult,
  MountFileReadResult,
  MountFileWriteResult,
} from '../../core/core-contract';

/**
 * Pascal's Workbench read/write surface (§W of the unit-12 ∥ P4.6bb round).
 *
 * The four authoring verbs are lane AY's (`customToolsLibrary` /
 * `customToolsDestinations` / `customToolPreview` / `customToolAudit`); their
 * response `type` strings are AY-owned, so every body is read STRUCTURALLY via
 * `dispatchData` (the autonomous-room / P4.6au precedent) rather than a pinned
 * response variant. The request verbs and the DTO shapes are what the wire
 * mirror diffs.
 *
 * **File I/O deliberately rides the EXISTING mount-file verbs** (§W2), exactly
 * as v4 does: "reads, writes, and deletes go through the existing mount-points
 * file routes, so the Workbench adds no second write path into stores"
 * (v4 `app/api/v1/custom-tools/route.ts`). Nothing here writes through a
 * Workbench-specific route, because there isn't one.
 *
 * @module screens/custom-tools/workbench.api
 */

export const workbenchKeys = {
  /** The library — invalidated after every save and delete. */
  library: () => ['customTools', 'library'] as const,
  /** The destination tree — the save/save-as picker's source. */
  destinations: () => ['customTools', 'destinations'] as const,
};

/** v4 `GET /api/v1/custom-tools` — every definition in every enabled store. */
export async function fetchLibrary(core: CoreClient): Promise<CustomToolLibraryResponse> {
  const data = await core.dispatchData({ type: 'customToolsLibrary' });
  return data as unknown as CustomToolLibraryResponse;
}

/** v4 `GET /api/v1/custom-tools?action=destinations` — the save-target tree. */
export async function fetchDestinations(core: CoreClient): Promise<CustomToolDestinations> {
  const data = await core.dispatchData({ type: 'customToolsDestinations' });
  return data as unknown as CustomToolDestinations;
}

/**
 * §B of the `616930db` round — the bench's scripted oracle.
 *
 * Typed HERE rather than in `core-contract.ts` because these are request-body
 * fields on two EXISTING verbs, not a new response shape; lane D8 owns the
 * server-side union and the `p4_6ay_workbench_wire_contract` diff.
 *
 * Preview takes all three arms. **The audit has no `live` arm, and that is
 * enforced by the SHAPE, not by a guard**: ten thousand hands must never mean
 * ten thousand paid consults. (Folded into `core-contract.ts`'s
 * `CustomToolPreviewRequest` / `CustomToolAuditRequest` at the round's
 * unification; these aliases keep the callers' vocabulary.)
 */
export type PreviewOracle = { live: true } | { output: string } | { fail: true };
export type AuditOracle = { output: string } | { fail: true };

/** The shared body of a bench run: the raw definition + optional params/sheet. */
export interface BenchArgs {
  /** The RAW document — the server validates and owns the rejection sentence. */
  definition: unknown;
  params?: Record<string, unknown> | null;
  /**
   * Either a hand-typed metadata object (passed through verbatim) or
   * `{ characterId }` (hydrated server-side). The bench NEVER resolves a sheet
   * itself — §W1.
   */
  metadata?: CustomToolMetadataInput | null;
}

/** v4 `POST ?action=preview` — one deal through the real execution core. */
export async function previewCustomTool(
  core: CoreClient,
  args: BenchArgs & { private?: boolean; llm?: PreviewOracle | undefined },
): Promise<CustomToolRunResult> {
  const data = await core.dispatchData({
    type: 'customToolPreview',
    definition: args.definition,
    params: args.params,
    private: args.private,
    metadata: args.metadata,
    llm: args.llm,
  });
  return data as unknown as CustomToolRunResult;
}

/**
 * v4 `POST ?action=audit` — ten thousand hands, and where they landed. The run
 * count is server-fixed (`AUDIT_RUNS`), so no `runs` field crosses the wire.
 */
export async function auditCustomTool(
  core: CoreClient,
  args: BenchArgs & { llm?: AuditOracle | undefined },
): Promise<CustomToolAuditResult> {
  const data = await core.dispatchData({
    type: 'customToolAudit',
    definition: args.definition,
    params: args.params,
    metadata: args.metadata,
    llm: args.llm,
  });
  return data as unknown as CustomToolAuditResult;
}

// --------------------------------------------------------------- §W2 file I/O

/** Read one definition file (the `readMountFile` JSON envelope). */
export async function readDefinitionFile(
  core: CoreClient,
  mountPointId: string,
  path: string,
): Promise<MountFileReadResult> {
  const data = await core.dispatchData({ type: 'mountFileRead', mountPointId, path });
  return data as unknown as MountFileReadResult;
}

/** What a save carries beyond the bytes: the conflict guard and its override. */
export interface WriteDefinitionArgs {
  mountPointId: string;
  path: string;
  content: string;
  /**
   * The mtime the editor loaded. Sent only for a save back to the SAME
   * location — a save-as or a first save has nothing to guard against.
   */
  expectedMtime?: number;
  /** The operator's decision to overwrite a file that moved underneath them. */
  force?: boolean;
}

/** Write one definition file (v4's item-route PUT). */
export async function writeDefinitionFile(
  core: CoreClient,
  args: WriteDefinitionArgs,
): Promise<MountFileWriteResult> {
  const data = await core.dispatchData({
    type: 'mountFileWrite',
    mountPointId: args.mountPointId,
    path: args.path,
    content: args.content,
    expectedMtime: args.expectedMtime,
    force: args.force,
  });
  return data as unknown as MountFileWriteResult;
}

/** Delete one definition file (v4's item-route DELETE). */
export async function deleteDefinitionFile(
  core: CoreClient,
  mountPointId: string,
  path: string,
): Promise<void> {
  await core.dispatchData({ type: 'mountFileDelete', mountPointId, path });
}

/**
 * Does a file already sit at this path? The save-as flow probes before writing
 * into a NEW path so an overwrite is a decision rather than an accident (v4
 * `WorkbenchEditor:262-270`).
 *
 * ONLY a not-found means "nothing there". v4 checks `error.status === 404` and
 * RETHROWS anything else, which matters: a transport failure read as "the path
 * is free" would turn the next write into a silent clobber.
 */
export async function definitionFileExists(
  core: CoreClient,
  mountPointId: string,
  path: string,
): Promise<boolean> {
  try {
    await readDefinitionFile(core, mountPointId, path);
    return true;
  } catch (err) {
    if (err instanceof CoreDispatchError && err.kind === 'not-found') return false;
    throw err;
  }
}
