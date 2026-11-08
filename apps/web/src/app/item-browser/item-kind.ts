/**
 * The two item browsers built on the shared folder layer: the Wardrobe and
 * the Inventory. A shared component or factory takes one of these and looks
 * up its wording and stores by it, rather than each kind keeping a copy — v4
 * `lib/folder-browser/item-kind.ts` at the pin `624d0d25c`.
 *
 * Client-safe: a type only.
 *
 * @module item-browser/item-kind
 */

export type ItemKind = 'wardrobe' | 'inventory';
