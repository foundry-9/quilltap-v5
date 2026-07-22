/**
 * The tabbed-workspace shared contract (p4.9j) — types + injection seams.
 *
 * Pre-committed at round setup (2026-07-19) and BINDING for both round lanes:
 * lane p4.9j1 (workspace core + chrome + shell) PROVIDES every token below;
 * lane p4.9j2 (screen hostability) INJECTS them `{ optional: true }` — a null
 * injection means "routed (legacy) mode; behave exactly as today". NEITHER
 * lane edits this file: a change here invalidates both work orders and is a
 * unifier-level event.
 *
 * The type shapes are a port of v4 `lib/workspace/types.ts` (baseline
 * `b8b12695`) and are corpus-locked by lane p4.9j1's tier-1 differential —
 * do not "improve" them; v4 is the oracle.
 */
import { InjectionToken, type Signal } from '@angular/core';

export type PaneId = 'left' | 'right';

/**
 * The kind of surface a tab renders — v4's list, verbatim (all 23 kinds).
 * `help` is intentionally absent (Help stays a modal in v4). `terminal` and
 * `document` are child tabs of a Salon tab (linked via `parentTabId`).
 * `wardrobe` is a tab only when opened from the left rail (the chat-scoped
 * path keeps its dialog). Kinds whose v5 surface is unported (`brahma`)
 * KEEP their slot here — the persistence layer must round-trip them — and
 * render a loud refusal pane in the host.
 */
export type TabKind =
  | 'home'
  | 'salon' // payload: SalonTabPayload
  | 'salon-list' // the all-chats list (`/salon` without a conversation)
  | 'terminal' // payload: TerminalTabPayload — child of a salon tab (Ariel)
  | 'document' // payload: DocumentTabPayload — child of a salon tab (Librarian)
  | 'aurora'
  | 'prospero'
  | 'scriptorium'
  | 'settings' // payload: SettingsTabPayload (deep-link target)
  | 'files'
  | 'photos'
  | 'scenarios'
  | 'brahma'
  | 'wardrobe' // payload: WardrobeTabPayload — RAIL-opened only; NO chatId
  | 'profile'
  | 'about'
  | 'generate-image'
  | 'document-standalone' // payload: DocumentStandaloneTabPayload
  | 'character-new'
  | 'character-edit' // payload: CharacterEditTabPayload
  | 'character-view' // payload: CharacterViewTabPayload
  | 'settings-wizard'
  | 'custom-tools' // payload: CustomToolsTabPayload (absent = library view)
  /**
   * **v5-ONLY** (P4.d16 tier 2) — payload: {@link SalonNewTabPayload}. v4 opens
   * New Chat as a MODAL (`open=new-chat` pops `NewChatModal`); v5 never ported
   * that modal — New Chat is a full screen at `/salon/new` (the standing
   * no-modal divergence, ruled in `p4.9j3-wardrobe-tab-riders.md`). Hosting
   * that screen as a tab is the faithful translation: the deep link stops
   * unmounting the workspace, and creating a chat self-closes the tab the way
   * the modal dismisses. This kind is NOT in v4's `TabKind` union, so it never
   * enters the captured corpus — see `core/workspace-core.spec.ts`.
   */
  | 'salon-new';

/** Kind-specific tab payloads (v4 `lib/workspace/types.ts`, verbatim shapes). */
export interface SalonTabPayload {
  chatId: string;
}
export interface TerminalTabPayload {
  chatId: string;
  sessionId?: string;
}
export interface DocumentTabPayload {
  chatId: string;
  /** Row id of the open chat_documents record this tab edits. Several document
   * tabs may share a chatId — one per open document. */
  chatDocumentId: string;
  /** Cached title for the tab label (the document's display title). */
  displayTitle?: string;
}
/**
 * A chat-less Document Mode tab (opened from the left rail / files surfaces).
 * The tab itself is the only record of the open — there is no chat_documents
 * row and no chat to notify of edits.
 */
export interface DocumentStandaloneTabPayload {
  /**
   * Client-minted identity key. For existing files the opener derives it from
   * the file's identity (scope/mount/path) so reopening the same file focuses
   * the existing tab; for new blank documents it's a fresh uuid. Stable across
   * renames (the payload's filePath updates, the key does not).
   */
  docKey: string;
  scope: 'document_store' | 'general';
  mountPoint?: string | null;
  /**
   * Unset while a brand-new blank document is being created; filled in via a
   * payload refresh once the server names it, so a persisted tab reopens the
   * real file.
   */
  filePath?: string;
  /** Folder (relative to scope root) for a new blank document. */
  targetFolder?: string;
  /** Cached title for the tab label. */
  displayTitle?: string;
}
/**
 * Optional drill-in payloads for the list surfaces. A deep link (e.g.
 * `/prospero/<id>`) opens the singleton tab with the target preselected; the
 * view renders that detail in place. Absent payload = the plain list.
 */
export interface ProsperoTabPayload {
  projectId?: string;
}
export interface ScriptoriumTabPayload {
  storeId?: string;
}
export interface AuroraTabPayload {
  groupId?: string;
}
export interface SettingsTabPayload {
  tab?: string;
  section?: string;
}
export interface WardrobeTabPayload {
  characterId?: string;
}
export interface CharacterEditTabPayload {
  characterId: string;
  /** Deep-link target sub-tab (e.g. `system-prompts`). */
  tab?: string;
}
export interface CharacterViewTabPayload {
  characterId: string;
  /** Deep-link target sub-tab (e.g. `conversations`). */
  tab?: string;
}
/**
 * Pascal's Workbench. No payload = the library view. A `mountPointId` + `path`
 * pair opens one definition in the builder (one tab per open definition, like
 * `character-edit`); `create` opens the builder on a fresh draft, with
 * `mountPointId` preselecting the destination.
 */
export interface CustomToolsTabPayload {
  mountPointId?: string;
  path?: string;
  create?: boolean;
}
/**
 * **v5-only** (P4.d16 tier 2). The three seeds v4 hands its NewChatModal
 * (`{ projectId?, characterId?, autonomous }`), carried as a tab payload
 * instead. Absent payload = a blank New Chat.
 */
export interface SalonNewTabPayload {
  characterId?: string;
  projectId?: string;
  autonomous?: boolean;
}

export interface WorkspaceTab {
  /** Stable uuid. */
  id: string;
  kind: TabKind;
  /** Kind-specific payload (e.g. `{ chatId }`). */
  payload?: unknown;
  /** Shown on the tab. */
  title: string;
  icon?: string;
  /** For terminal/document: the salon tab they belong to. */
  parentTabId?: string;
}

export interface PaneState {
  /** Tab ids in display order. */
  order: string[];
  /** The visible tab in this pane. */
  activeTabId: string | null;
}

export interface WorkspaceState {
  tabs: Record<string, WorkspaceTab>;
  panes: {
    left: PaneState;
    /** `null` = unsplit (single full-width pane). */
    right: PaneState | null;
  };
  /** Last-interacted pane; new rail-opened tabs land here. Default `'left'`. */
  focusedPane: PaneId;
  /** Left pane fraction of width when split (0..1). Persisted. */
  splitRatio: number;
}

/** Default split ratio (panes evenly split). v4 `types.ts`. */
export const DEFAULT_SPLIT_RATIO = 0.5;
/**
 * Min/max left-pane fraction so neither pane becomes unusably narrow —
 * clamping happens in the reducer (`SET_SPLIT_RATIO`), pixel clamping in the
 * divider drag handler. v4 `types.ts`.
 */
export const MIN_SPLIT_RATIO = 0.2;
export const MAX_SPLIT_RATIO = 0.8;

/**
 * Identity key for a standalone document tab (v4 `standaloneDocKey`,
 * verbatim semantics). Existing files key by identity (scope/mount/path) so
 * reopening the same file focuses its existing tab; a new blank document (no
 * filePath yet) gets a fresh uuid so several blanks can coexist.
 */
export function standaloneDocKey(
  scope: DocumentStandaloneTabPayload['scope'],
  mountPoint: string | null | undefined,
  filePath: string | null | undefined,
): string {
  if (filePath) return `${scope}:${mountPoint ?? ''}:${filePath}`;
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `doc-${Math.random().toString(36).slice(2)}${Math.random().toString(36).slice(2)}`;
}

/**
 * Portal key for a chat-linked child pane (terminal/document) — v4
 * `portalKey`, verbatim. Terminal is one per chat; documents are keyed
 * additionally by the open document's row id so a chat can portal several
 * document panes (one per open document) at once.
 */
export function portalKey(kind: 'terminal' | 'document', chatId: string, docId?: string): string {
  return docId ? `${kind}:${chatId}:${docId}` : `${kind}:${chatId}`;
}

// ---------------------------------------------------------------------------
// Injection seams — lane p4.9j1 provides, lane p4.9j2 injects (optional)
// ---------------------------------------------------------------------------

export interface OpenTabOptions {
  pane?: PaneId;
  focus?: boolean;
  title?: string;
  icon?: string;
  parentTabId?: string;
}

/**
 * The workspace store's screen-facing surface (v4 `useWorkspace`, reduced to
 * what hosted screens legitimately call). Provided at the workspace level;
 * null outside the workspace (routed mode).
 */
export interface WorkspaceHandle {
  /**
   * Open (or focus an existing) tab. De-dupes by kind+payload identity.
   * Returns the resulting tab id (the existing one when de-duped).
   */
  openTab(kind: TabKind, payload?: unknown, opts?: OpenTabOptions): string;
  closeTab(id: string): void;
  /**
   * Refresh an open tab's payload/title in place (the v4 `OPEN_TAB`
   * focus:false payload-refresh path — e.g. a blank standalone document
   * receiving its real filePath after the server names it).
   */
  refreshTab(kind: TabKind, payload: unknown, title?: string): void;
}
export const WORKSPACE_HANDLE = new InjectionToken<WorkspaceHandle>('quilltap.workspace.handle');

/**
 * The hosting tab's id — provided PER TAB by the host around each mounted
 * tab's view (v4 `useWorkspaceTabId`). Null ⇒ the component is rendered by
 * the router (legacy mode) and must behave exactly as today.
 */
export const WORKSPACE_TAB_ID = new InjectionToken<string>('quilltap.workspace.tabId');

/**
 * Cross-tab portal registry (v4 `WorkspacePortalRegistryProvider`): a child
 * tab (terminal/document) registers its DOM host node under `portalKey(...)`;
 * the owning Salon view reads the node and physically relocates its live pane
 * element into it (the DOM move preserves the PTY/editor state; the
 * component stays in the Salon's logical tree).
 */
export interface WorkspacePortalRegistry {
  setNode(key: string, node: HTMLElement | null): void;
  /** Reactive view of the registered nodes (read `nodes()[key]`). */
  readonly nodes: Signal<Readonly<Record<string, HTMLElement | null>>>;
}
export const WORKSPACE_PORTAL_REGISTRY = new InjectionToken<WorkspacePortalRegistry>(
  'quilltap.workspace.portalRegistry',
);

/** One tab's reported backdrop (v4 `BackdropEntry`). */
export interface WorkspaceBackdropEntry {
  url: string;
  isSalon: boolean;
}
/**
 * The workspace backdrop registry (v4 `WorkspaceBackdropProvider`): views
 * report their story/subsystem background; the host paints one arbitrated
 * backdrop (a Salon with a background wins full-screen; otherwise unsplit →
 * the single active tab's; split → per-side with a crossfade at the divider).
 * Report with the OWN tab id from WORKSPACE_TAB_ID; clear on url-null and on
 * destroy.
 */
export interface WorkspaceBackdropRegistry {
  report(tabId: string, entry: WorkspaceBackdropEntry): void;
  clear(tabId: string): void;
}
export const WORKSPACE_BACKDROP_REGISTRY = new InjectionToken<WorkspaceBackdropRegistry>(
  'quilltap.workspace.backdropRegistry',
);
