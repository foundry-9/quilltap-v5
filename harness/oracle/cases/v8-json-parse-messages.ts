/**
 * P4.154 tier-1 oracle — V8's REAL `JSON.parse` failure wording, the text v4
 * logs wherever it catches a parse failure and renders `err.message` (the
 * document-store overlay's `properties.json unparseable: ${err.message}`,
 * `parseLLMJson`'s propagated `SyntaxError`, the SDKs' body parse). v5
 * reproduces it in `quilltap_core::generators::optimizer::
 * v8_json_parse_message`; this case RECORDS it — nothing here is typed by
 * hand.
 *
 * Every row is `{kind:'parse', id, input, message}` where `message` is the
 * thrown `SyntaxError`'s `.message`, or `null` when V8 ACCEPTS the input (the
 * twin must then answer `None`). A leading `{kind:'node', version}` row names
 * the Node that recorded it: the wording is V8's, so a different Node is a
 * different oracle (the Rust side asserts `v24.13.1`).
 *
 * The corpus covers the start-of-input shapes the twin always reproduced (the
 * P4.9K1 measured table, re-recorded here), the INSIDE-a-value shapes it
 * learned in P4.154 (dogfood #146: `{` → `Expected property name or '}' …`),
 * every templated V8 message, the `(line L column C)` suffix over `\n`, `\r`
 * and `\r\n`, the 21-unit context window at every placement, UTF-16 units
 * (astral and BMP non-ASCII), and the special whole-source strings.
 *
 * Run from inside the v4 checkout (or a pinned worktree) — the case imports
 * nothing from v4, but the Node is the recording instrument:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/v8-json-parse-messages.ts \
 *     > /tmp/oracle-v8-json-parse-messages.ndjson
 */

const rows: unknown[] = [{ kind: 'node', version: process.version }];

const long = 'x'.repeat(30);

const inputs: Array<[string, string]> = [
  // --- start-of-input shapes (the P4.9K1 measured table) ---
  ['prose-long', 'The character is fine as she is.'],
  ['prose-short', 'I have no'],
  ['abc', 'abc'],
  ['single-A', 'A'],
  ['empty', ''],
  ['ws-only', '   \n'],
  ['prose-then-array', 'Sure! Here is the JSON you asked for: []'],
  ['ws-then-prose', '\n\n\t  Sure thing, here it is: {}'],
  ['indented-prose', '            The character'],
  ['x21', 'x'.repeat(21)],
  ['x20', 'x'.repeat(20)],
  ['dot-five', '.5'],
  ['tru', 'tru'],
  ['n', 'n'],
  ['nul', 'nul'],
  ['fals', 'fals'],
  ['no-json-here', 'no json here'],
  ['nope', 'nope'],
  ['nonsense-long', 'nonsense that is quite long indeed'],
  ['ws-nah', '    nah, not json at all here'],
  ['null-x', 'null x'],
  ['truex', 'truex'],
  ['false-bang', 'false!'],
  ['nl-null-x', '\nnull x'],
  ['nl-true-bang', '\n\n  true  !'],
  ['null-nl-nl-x', 'null\n\nx'],
  ['twelve-ab', '12ab'],
  // --- special whole-source strings ---
  ['undefined', 'undefined'],
  ['NaN', 'NaN'],
  ['Infinity', 'Infinity'],
  ['object-Object', '[object Object]'],
  ['undefined-ws', ' undefined'],
  ['undefinedx', 'undefinedx'],
  // --- accepted inputs (the twin answers None) ---
  ['ok-null', 'null'],
  ['ok-object', '{"a":1}'],
  ['ok-nested', ' {"a":[1,2,{"b":null}],"c":"d"} '],
  ['ok-number', '-0.5e+10'],
  ['ok-string', '"esc \\" \\\\ \\/ \\b \\f \\n \\r \\t \\u00e9"'],
  ['ok-empty-object', '{ }'],
  ['ok-empty-array', '[ ]'],
  // V8 ACCEPTS these where serde refuses — the twin's whole `None` scope
  // after a serde failure (the caller's recorded fallback to serde's text).
  ['ok-number-past-f64', '{"a":1e400}'],
  ['ok-lone-surrogate-escape', '{"a":"\\ud800"}'],
  ['ok-deep-nesting', '['.repeat(200) + ']'.repeat(200)],
  // --- inside an object ---
  ['lbrace', '{'],
  ['lbrace-ws', '{  '],
  ['key-only', '{"a"'],
  ['key-colon', '{"a":'],
  ['key-colon-ws', '{"a": '],
  ['value-comma', '{"a":1,'],
  ['trailing-comma-obj', '{"a":1,}'],
  ['missing-comma-obj', '{"a":1 "b":2}'],
  ['bare-key', '{a:1}'],
  ['single-quoted-key', "{'a':1}"],
  ['number-key', '{1:2}'],
  ['missing-colon', '{"a" 1}'],
  ['missing-colon-eos-ws', '{"a"   '],
  ['bad-literal-value', '{"a":tru}'],
  ['bad-literal-value-x', '{"a":trux}'],
  // The `94fbb1ae3` smalls unification's review (an independent fuzz against
  // real Node found these two classes the lane's corpus missed): V8's
  // `ScanLiteral` reports the mismatching unit's TOKEN TYPE — a digit or `-`
  // reads `Unexpected number`, a `"` reads `Unexpected string` …
  ['literal-broken-by-digit', 't1'],
  ['literal-broken-by-minus', 't-'],
  ['literal-broken-by-digit-in-value', '{"a":nul1}'],
  ['literal-broken-by-quote', '[fals"]'],
  ['bare-word-value', '{"a":x}'],
  ['extra-rbrace', '{"a":1}}'],
  ['obj-then-rbrack', '{"a":1]'],
  ['comma-after-lbrace', '{,}'],
  ['colon-after-lbrace', '{:}'],
  ['bare-key-after-comma', '{"a":1,b:2}'],
  ['value-eos', '{"a":1'],
  ['value-eos-ws', '{"a":1  '],
  ['nested-unclosed', '{"a":{"b":[1,2'],
  // --- inside an array ---
  ['lbrack', '['],
  ['lbrack-ws', '[   '],
  ['elem-comma', '[1,'],
  ['missing-comma-arr', '[1 2]'],
  ['trailing-comma-arr', '[1,]'],
  ['double-comma-arr', '[1,,2]'],
  ['leading-comma-arr', '[,1]'],
  ['arr-then-rbrace', '[1}'],
  ['arr-eos', '[1,2'],
  ['arr-colon', '[1:2]'],
  ['arr-bare-word', '[1,x]'],
  ['arr-string-then-string', '["a" "b"]'],
  // --- strings ---
  ['unterminated', '"unterminated'],
  ['unterminated-in-obj', '{"a":"b'],
  ['unterminated-key', '{"ab'],
  ['bad-escape', '"a\\x"'],
  ['bad-escape-at-end', '"a\\'],
  // … and an escape of a unit above U+00FF is not `Bad escaped character`
  // but the context-window token (a model escaping a curly quote, `\’`, is
  // the realistic source on the `parseLLMJson` path); Latin-1 still reads
  // `Bad escaped character`.
  ['escape-of-latin1', '"\\é"'],
  ['escape-of-u0100', '"\\Ā"'],
  ['escape-of-curly-quote', '{"a":"\\’hi\\’"}'],
  ['escape-of-u2028', '"\\\u2028"'],
  ['escape-of-astral', '"\\😀"'],
  ['bad-unicode-escape', '"\\u12G4"'],
  ['short-unicode-escape', '"\\u12"'],
  ['control-char-newline', '"a\nb"'],
  ['control-char-tab', '"a\tb"'],
  ['control-char-in-key', '{"a\u0001":1}'],
  ['single-quoted-string', "'a'"],
  ['unicode-escape-eos', '"\\u12'],
  ['unicode-escape-eos-bare', '"\\u'],
  ['escape-then-eos-in-key', '{"a\\'],
  ['ok-escaped-key', '{"a\\nb":1}'],
  ['control-char-del-ok', '"a\u007fb"'],
  // --- numbers ---
  ['minus-alone', '-'],
  ['minus-x', '-x'],
  ['minus-ws', '- 1'],
  ['leading-zero', '01'],
  ['leading-zero-in-arr', '[01]'],
  ['neg-leading-zero', '-01'],
  ['dot-no-fraction', '1.'],
  ['dot-x', '1.x'],
  ['exp-no-digits', '1e'],
  ['exp-sign-no-digits', '1e+'],
  ['exp-x', '1ex'],
  ['plus-number', '+1'],
  ['hex-number', '0x10'],
  ['number-then-word', '12abc'],
  ['two-numbers', '1 2'],
  ['number-in-obj-unterminated-fraction', '{"a":1.}'],
  ['dot-start-in-arr', '[.5]'],
  ['minus-dot', '-.5'],
  ['double-zero', '00'],
  ['fraction-exp-no-digits', '1.5e'],
  ['ok-upper-exp', '1E5'],
  ['ok-neg-zero', '-0'],
  ['fraction-then-word', '1.5x'],
  ['dot-no-fraction-in-arr', '[1.]'],
  ['dot-then-exp', '1.e5'],
  ['exp-minus-x', '1e-x'],
  // --- after the top-level value ---
  ['obj-then-obj', '{}{}'],
  ['array-then-word', '[] x'],
  ['string-then-string', '"a" "b"'],
  // --- line / column suffix ---
  ['multiline-lbrace', '{\n'],
  ['multiline-missing-comma', '{\n  "a": 1\n  "b": 2\n}'],
  ['multiline-trailing-comma', '[\n  1,\n  2,\n]'],
  ['cr-line', '{\r"a" 1}'],
  ['crlf-line', '{\r\n"a" 1}'],
  ['crlf-two-lines', '[\r\n1\r\n2]'],
  ['cr-cr', '[\r\r1 2]'],
  ['cr-before-eos', '{\r'],
  ['crlf-before-eos', '{\r\n'],
  ['cr-then-lf-separate', '[\r 1\n 2]'],
  ['nl-in-string-then-fault', '{"a":"x",\n"b" 2}'],
  // --- the 21-unit context window inside a value ---
  ['window-start', '{"a":x' + long + '}'],
  ['window-middle', '{"' + long + '":x' + long + '}'],
  ['window-end', '{"' + long + '":x}'],
  ['window-edge-20', '{"aaaaaaaaaaaaaa":x}'],
  ['window-edge-21', '{"aaaaaaaaaaaaaaa":x}'],
  ['long-trailing-comma', '{"' + long + '":1,}'],
  ['long-unterminated', '"' + long],
  // --- UTF-16 units ---
  ['bmp-key-fault', '{"é":1 "b":2}'],
  ['astral-key-fault', '{"😀":1 "b":2}'],
  ['astral-bare-token', '😀'],
  ['astral-in-window', '{"😀😀😀😀😀😀😀😀😀😀😀😀😀😀":x}'],
  ['bmp-bare-token', 'é'],
  ['astral-value-token', '{"a":😀}'],
  // --- the overlay's own shape (a truncated properties.json) ---
  ['properties-truncated', '{\n  "name": "Foundry-9",\n  "description": "A wo'],
  ['properties-missing-comma', '{\n  "name": "Foundry-9"\n  "color": null\n}'],
];

for (const [id, input] of inputs) {
  let message: string | null = null;
  try {
    JSON.parse(input);
  } catch (err) {
    if (!(err instanceof SyntaxError)) throw err;
    message = err.message;
  }
  // A lone surrogate (a token or window edge that splits an astral pair)
  // cannot cross NDJSON into a Rust `String`; `wellFormed: false` marks the
  // row and `message` carries V8's text with each lone unit as U+FFFD.
  const wellFormed = message === null || message.isWellFormed();
  rows.push({
    kind: 'parse',
    id,
    input,
    message: message === null ? null : message.toWellFormed(),
    wellFormed,
  });
}

for (const row of rows) process.stdout.write(JSON.stringify(row) + '\n');
