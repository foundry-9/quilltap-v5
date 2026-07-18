/**
 * Non-code assets imported as TEXT, typed.
 *
 * `.ndjson` is registered as an esbuild `text` loader in angular.json's build
 * options, so a committed oracle corpus reaches a jsdom spec as a string that is
 * byte-identical to the file on disk — which is what keeps the fixture a copy
 * rather than something a loader reshaped on the way in.
 */
declare module '*.ndjson' {
  const content: string;
  export default content;
}
