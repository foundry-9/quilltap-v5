/**
 * Derived per-mount capability flags — the SPA twin of v4
 * `lib/mount-index/capabilities.ts` `deriveMountCapabilities` (unmoved in the
 * span; ported verbatim at the pin `624d0d25c`, P4.D387 item 3).
 *
 * v4 folds the result into the single-mount GET only; its store catalogue
 * (`DocumentDestinationDialog`'s `useStoreCatalogue`) derives it client-side
 * for every listed store. v5's list rows carry no `capabilities` either
 * (`api/mount_points.rs` adds them on `mountPointGet` alone), so the
 * destination catalogue needs this twin.
 *
 * Rules (v4's docblock):
 * - A mount mid-conversion (`conversionStatus` of `converting` /
 *   `deconverting`) is quiesced: no writes, deletes, folder creates, or
 *   move/copy in either direction, and no fresh conversion may be kicked off.
 * - A disabled mount allows nothing.
 * - `canConvert` additionally refuses while a scan is running, mirroring the
 *   guards enforced server-side in `handleConvert` / `handleDeconvert`.
 *
 * Pure; `document-browser.oracle.spec.ts` replays v4's REAL function.
 */

import type { MountPointCapabilities } from '../core/core-contract';

/** What the derivation reads off a mount point. */
export interface MountCapabilityInput {
  enabled: boolean;
  conversionStatus: string;
  scanStatus: string;
}

export function deriveMountCapabilities(mp: MountCapabilityInput): MountPointCapabilities {
  const midConversion =
    mp.conversionStatus === 'converting' || mp.conversionStatus === 'deconverting';
  const quiescent = mp.enabled && !midConversion;

  return {
    canWrite: quiescent,
    canDelete: quiescent,
    canCreateFolder: quiescent,
    canMoveIn: quiescent,
    canMoveOut: quiescent,
    canConvert: quiescent && mp.scanStatus !== 'scanning',
  };
}
