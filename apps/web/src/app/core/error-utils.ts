/**
 * v4 `getErrorMessage` (`lib/error-utils.ts:19-33`), transcribed: an Error's
 * message (even an empty one); a string as itself; any other non-nullish value
 * stringified unless it reads `[object Object]`; else `fallback`.
 */
export function getErrorMessage(error: unknown, fallback = 'Unknown error'): string {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === 'string') {
    return error;
  }
  if (error !== null && error !== undefined) {
    const stringified = String(error);
    if (stringified !== '[object Object]') {
      return stringified;
    }
  }
  return fallback;
}
