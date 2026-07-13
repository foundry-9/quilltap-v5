/**
 * Local types for the `qt-file-manager` file surface (lane B, work order
 * P4.6aa). The component contract (§2) requires lane B to define
 * `MountCapabilities` **locally** here; lane A types the server `capabilities`
 * bag in `core-contract.ts` with the SAME shape. The unifier MAY dedupe (flip
 * this import to core-contract) but MUST NOT reshape.
 *
 * @module files/types
 */

/**
 * Per-mount capability flags — pinned verbatim from v4
 * `lib/mount-index/capabilities.ts:32`. The SERVER derives these
 * (`mountPointGet` echoes a `capabilities` bag); the widget consumes, never
 * re-derives, them.
 */
export interface MountCapabilities {
  /** Upload / save / write into this mount. */
  canWrite: boolean;
  /** Delete files or folders in this mount. */
  canDelete: boolean;
  /** Create new folders in this mount. */
  canCreateFolder: boolean;
  /** This mount may be the *destination* of a copy/move. */
  canMoveIn: boolean;
  /** This mount may be the *source* of a copy/move. */
  canMoveOut: boolean;
  /** Offer the convert (fs→db) or deconvert (db→fs) action right now. */
  canConvert: boolean;
}

/** The three mount kinds the component is told about (contract §2). */
export type MountType = 'filesystem' | 'obsidian' | 'database';

/**
 * One tree node the widget renders. Identity is the **mount-relative path**
 * (`/`-separated, no leading slash; `''` = the mount root) — path-native, not
 * SVAR's opaque numeric id (the bespoke build's simplification, D18 spike).
 */
export interface FileNode {
  /** Mount-relative path; `''` for the root. */
  path: string;
  type: 'file' | 'folder';
  /** File byte size (files only). */
  size?: number;
  /** Last-modified timestamp (files only). */
  date?: Date;
}
