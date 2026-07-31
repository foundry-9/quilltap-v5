import { Injectable, signal } from '@angular/core';
import { Subject, type Observable } from 'rxjs';

import {
  type CoreRequest,
  type CoreResponse,
  type ScopedEvent,
  CoreDispatchError,
  expectResponse,
  type ResponseType,
  type AutonomousRoomSummary,
  type AutonomousRoomStatusDto,
  type AutonomousRoomSettingsPatch,
  type AutonomousRoomUpdateResult,
  type DataRetentionSettingsDto,
  type FileEntry,
  type FolderEntry,
} from './core-contract';
import {
  type ConnectionState,
  type CoreTransport,
  type EventStreamSink,
  type HealthStatus,
} from './core-transport';
import { createCoreTransport } from './core-transport-factory';

// The interpreted shapes moved to core-transport.ts with the P4.7b transport
// split; re-exported so the pre-split import sites stay valid.
export { parseEventData } from './core-transport';
export type { ConnectionState, HealthStatus } from './core-transport';

/**
 * Pull a human-readable message out of a thrown value — v5's analog of v4
 * `apiErrorMessage()` in `lib/query/fetcher.ts` (extracted by `0246c6c8`), and
 * here for the same reason: beside the error it reads.
 *
 * v4's helper also unwraps an `ApiFetchError`'s parsed `{ error }` body, because
 * its dialogs hold the raw fetch failure. v5 has no analog of that half:
 * `CoreDispatchError` is constructed FROM the `{ type: "error" }` envelope, so
 * its `message` is already the sentence a person should read. What is left is
 * v4's tail — an `Error` yields its own message, anything else the caller's
 * `fallback`, which is where each surface says what it could not do.
 */
export function coreErrorMessage(err: unknown, fallback: string): string {
  if (err instanceof Error) return err.message;
  return fallback;
}

/**
 * The ONE transport seam (D14): the whole server surface behind a single
 * injectable. Components never touch `fetch` / `EventSource` / Tauri IPC
 * directly — since P4.7b the raw touchpoints live in an internal
 * {@link CoreTransport} pair (HTTP for the browser, IPC inside the Tauri
 * shell), selected once at construction; everything in this class sits above
 * that boundary and behaves identically on both.
 *
 *  - {@link dispatch} → the typed action route (`POST /api/dispatch` / the
 *    `dispatch` command).
 *  - {@link events$} → ONE global event stream per app instance
 *    (`GET /api/events` / the `quilltap://event` Tauri channel).
 *  - {@link fetchHealth} → the `GET /health` readiness vocabulary (startup gate).
 */
@Injectable({ providedIn: 'root' })
export class CoreClient {
  /** The raw-IO pair, chosen at bootstrap (before first injection resolves). */
  private readonly transport: CoreTransport = createCoreTransport();

  private readonly frames = new Subject<ScopedEvent>();

  /** Parsed scope-tagged frames off the single global stream. */
  readonly events$: Observable<ScopedEvent> = this.frames.asObservable();

  /** The live-stream connection state (signal — drives the shell indicator). */
  readonly connection = signal<ConnectionState>('idle');

  /**
   * A monotonically increasing resync counter. Each time the stream signals a
   * possible gap — the EventSource REOPENING after an error over HTTP, a
   * `quilltap://resync` frame under Tauri — this bumps; consumers treat the
   * change as a "you may have missed frames, refetch authoritative state"
   * signal (D3 best-effort: no server-side replay buffer this round).
   */
  readonly resyncCounter = signal(0);

  /** The transports' narrow window onto the signals above (one shared frame path). */
  private readonly sink: EventStreamSink = {
    connectionState: () => this.connection(),
    setConnection: (state) => this.connection.set(state),
    bumpResync: () => this.resyncCounter.update((n) => n + 1),
    acceptFrame: (frame) => this.frames.next(frame),
  };

  // -------------------------------------------------------------------------
  // Dispatch
  // -------------------------------------------------------------------------

  /**
   * Send one request over the transport. ALWAYS resolves to a typed
   * `CoreResponse` — a non-2xx body is still the typed envelope (the Locked 503
   * carries the error envelope plus v4's `setupUrl` body, which we ignore here);
   * a transport failure becomes a synthetic `internal` error so callers get a
   * uniform shape (use {@link dispatchExpect} to throw instead).
   */
  async dispatch(request: CoreRequest): Promise<CoreResponse> {
    return this.transport.dispatch(request);
  }

  /**
   * Dispatch and narrow to the expected response variant, throwing a
   * {@link CoreDispatchError} on a `{ type: "error" }` envelope (or a plain
   * `Error` on a variant mismatch).
   */
  async dispatchExpect<T extends ResponseType>(
    request: CoreRequest,
    expect: T,
  ): Promise<Extract<CoreResponse, { type: T }>> {
    const resp = await this.dispatch(request);
    return expectResponse(resp, expect);
  }

  /**
   * Dispatch and return the raw `data` body of any non-error response, throwing a
   * {@link CoreDispatchError} on `{ type: "error" }`. Use for ops whose response
   * `type` string is not pinned by the Shared contract (the provider-test
   * actions) — only their `data` shape is load-bearing.
   */
  async dispatchData(request: CoreRequest): Promise<Record<string, unknown>> {
    const resp = await this.dispatch(request);
    if (resp.type === 'error') {
      throw new CoreDispatchError(resp.data);
    }
    return (resp.data ?? {}) as Record<string, unknown>;
  }

  // -------------------------------------------------------------------------
  // Autonomous rooms (P4.6ad) — the run-control surface behind one opaque
  // `autonomousRoom` response; each verb casts the `data` body it expects.
  // -------------------------------------------------------------------------

  /** v4 `GET /system/autonomous-rooms` — the user's rooms, running-first. */
  async listAutonomousRooms(): Promise<AutonomousRoomSummary[]> {
    const data = await this.dispatchData({ type: 'systemAutonomousRooms' });
    return (data['rooms'] as AutonomousRoomSummary[] | undefined) ?? [];
  }

  /** The per-room status snapshot (the editor modal's initial load). */
  async autonomousRoomStatus(chatId: string): Promise<AutonomousRoomStatusDto> {
    const data = await this.dispatchData({ type: 'chatAutonomousRoomStatus', chatId });
    return data as unknown as AutonomousRoomStatusDto;
  }

  /** Manually start a run → `{runId, jobId}`. */
  async autonomousRoomStart(chatId: string): Promise<{ runId: string; jobId: string }> {
    const data = await this.dispatchData({ type: 'chatAutonomousRoomStart', chatId });
    return data as unknown as { runId: string; jobId: string };
  }

  /** Pause the current run. */
  async autonomousRoomPause(chatId: string): Promise<void> {
    await this.dispatchData({ type: 'chatAutonomousRoomPause', chatId });
  }

  /** Stop the current run (bumps `currentRunId`). */
  async autonomousRoomStop(chatId: string): Promise<void> {
    await this.dispatchData({ type: 'chatAutonomousRoomStop', chatId });
  }

  /** Resume (or fresh-start) a paused/idle room → `{runId, jobId}`. */
  async autonomousRoomResume(chatId: string): Promise<{ runId: string; jobId: string }> {
    const data = await this.dispatchData({ type: 'chatAutonomousRoomResume', chatId });
    return data as unknown as { runId: string; jobId: string };
  }

  /** Apply the Edit-Enclave modal patch → `{updated, clampedDestructive}`. */
  async autonomousRoomUpdateSettings(
    chatId: string,
    settings: AutonomousRoomSettingsPatch,
  ): Promise<AutonomousRoomUpdateResult> {
    const data = await this.dispatchData({
      type: 'chatAutonomousRoomUpdateSettings',
      chatId,
      settings,
    });
    return {
      updated: data['updated'] === true,
      clampedDestructive: data['clampedDestructive'] === true,
    };
  }

  // -------------------------------------------------------------------------
  // Health (the startup readiness gate)
  // -------------------------------------------------------------------------

  /**
   * Read `GET /health` and interpret v4's status vocabulary (health.rs): 200
   * healthy, 423 locked (carries `dbKeyState`), 409 lock-conflict, 503
   * unhealthy, and an unreachable server (still booting) → retry.
   */
  async fetchHealth(): Promise<HealthStatus> {
    return this.transport.fetchHealth();
  }

  // -------------------------------------------------------------------------
  // The single global event stream
  // -------------------------------------------------------------------------

  /** Open the one live stream (idempotent). No-op where the transport can't. */
  connect(): void {
    this.transport.connect(this.sink);
  }

  /** Close the stream (the shell calls this on lock / teardown). */
  disconnect(): void {
    this.transport.disconnect();
    this.connection.set('closed');
  }

  // -------------------------------------------------------------------------
  // The general files family (P4.6af) — reads + the fire-and-forget thumbnail
  // batch. Mutations (move / promote / delete / folder CRUD / cleanup) ride
  // `dispatch`/`dispatchData` directly in the FileBrowser so it can inspect the
  // associations envelope and the loud refusals; only the read helpers live here.
  // -------------------------------------------------------------------------

  /** v4 `GET /files?filter=general` — the general-files list (`createdAt` DESC). */
  async filesList(opts: { projectId?: string; folderPath?: string; filter?: string } = {}): Promise<
    FileEntry[]
  > {
    const data = await this.dispatchData({ type: 'filesList', ...opts });
    return (data['files'] as FileEntry[] | undefined) ?? [];
  }

  /** v4 `GET /files/folders` — the DB folder rows (`path` localeCompare ASC). */
  async filesFoldersList(projectId?: string): Promise<FolderEntry[]> {
    const data = await this.dispatchData({ type: 'filesFoldersList', projectId });
    return (data['folders'] as FolderEntry[] | undefined) ?? [];
  }

  /**
   * v4 `POST ?action=generate-thumbnails` for the first 100 image ids — the
   * fire-and-forget batch fired after the list loads. Swallows failures (v4's
   * `.catch(() => {})` idiom): the thumbnails degrade to the on-demand path.
   */
  async filesGenerateThumbnails(fileIds: string[], size?: number): Promise<void> {
    if (fileIds.length === 0) return;
    try {
      await this.dispatchData({
        type: 'filesGenerateThumbnails',
        fileIds: fileIds.slice(0, 100),
        size,
      });
    } catch {
      /* best-effort — the thumbnail route regenerates on demand */
    }
  }

  // === P4.d3 (lane D): the data-retention setting ===
  // Single-author block; the file owner (lane B) must not edit it. Read
  // defensively via `dispatchData` (the settings precedent — no CoreResponse
  // variant). The interfaces + DTO live in core-contract.ts's P4.d3 block.

  /** v4 GET `/settings/data-retention` → the stale-chat window `{staleChatDays}`. */
  async getDataRetentionSettings(): Promise<DataRetentionSettingsDto> {
    const data = await this.dispatchData({ type: 'dataRetentionSettings' });
    return data as unknown as DataRetentionSettingsDto;
  }

  /** v4 PUT `/settings/data-retention` → the parsed echo (throws on validation). */
  async updateDataRetentionSettings(staleChatDays: number): Promise<DataRetentionSettingsDto> {
    const data = await this.dispatchData({ type: 'dataRetentionSettingsUpdate', staleChatDays });
    return data as unknown as DataRetentionSettingsDto;
  }
}
