/**
 * jsdom implements `Element.prototype.getClientRects`/`getBoundingClientRect`
 * but NOT `Range`'s — and `prosemirror-view`'s `coordsAtPos`/`singleRect`
 * calls `target.getClientRects()` on a `Range` while computing where to
 * scroll a transaction tagged `.scrollIntoView()` (`sinkListItem`/
 * `liftListItem`, exercised by the P4.D40 Tab/toolbar specs, and the
 * pre-existing Shift-Enter `insertBreak` call this). Importing this module
 * for its side effect stubs both to jsdom's own zeroed-rect shape, matching
 * how a real (non-empty) browser `Range` answers them, so the scroll math
 * no-ops harmlessly instead of throwing — a real DOM never hits this gap.
 *
 * @module editor/jsdom-range-shim
 */
if (typeof Range !== 'undefined' && !Range.prototype.getClientRects) {
  Range.prototype.getClientRects = function (): DOMRectList {
    return {
      length: 0,
      item: () => null,
      [Symbol.iterator]: () => [][Symbol.iterator](),
    } as unknown as DOMRectList;
  };
  Range.prototype.getBoundingClientRect = function (): DOMRect {
    return {
      x: 0,
      y: 0,
      width: 0,
      height: 0,
      top: 0,
      right: 0,
      bottom: 0,
      left: 0,
      toJSON: () => '',
    } as DOMRect;
  };
}
