/**
 * The Host's portrait (v4 `STAFF_AVATARS.host ?? '/images/avatars/host-avatar.webp'`
 * at `d1c06cd9d`), used by the dialog and by the two "Ask the Host" buttons.
 *
 * Its own module on purpose: the entry points `@defer` the dialog (v4 loads it
 * with `next/dynamic`), and importing any VALUE from the dialog's module would
 * make it an eager dependency and pull it back into the surface's bundle.
 *
 * @module scenario-builder/host-avatar
 */
export const HOST_AVATAR = '/images/avatars/host-avatar.webp';
