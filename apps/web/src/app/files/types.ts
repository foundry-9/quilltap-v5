/**
 * Types for the `qt-file-manager` file surface (work order P4.6aa). The
 * component contract (§2) had lane B define `MountCapabilities` locally; at
 * unification it was deduped to core-contract's identically-shaped
 * `MountPointCapabilities` (the sanctioned flip — shape unchanged).
 *
 * @module files/types
 */

import type { MountPointCapabilities } from '../core/core-contract';

/**
 * Per-mount capability flags — v4 `lib/mount-index/capabilities.ts:32`. The
 * SERVER derives these (`mountPointGet` echoes a `capabilities` bag); the
 * widget consumes, never re-derives, them.
 */
export type MountCapabilities = MountPointCapabilities;

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
