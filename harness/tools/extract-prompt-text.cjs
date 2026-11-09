// Mechanical regen of crates/quilltap-core/src/services/context_summary/prompt_text.rs:
// evaluate each named template literal from v4's chat-tasks.ts and splice its
// runtime value into the existing file's r#"…"# bodies (header + order kept).
//
// Usage: node extract-prompt-text.cjs <chat-tasks.ts> <prompt_text.rs> [<established-fact.ts>]
//
// P4.D372 (v4 `db6190b44`, R-L4): a template body may carry ONE kind of
// interpolation — `${NAME}` where NAME is in ALLOWED_INTERPOLATIONS and the
// optional third argument names the module whose `export const NAME =` right-
// hand side is a `+`-concatenation of plain single- or double-quoted string
// literals. That right-hand side is evaluated by a LITERAL-ONLY tokenizer (no
// `eval` of arbitrary code); anything else still throws `interpolation in
// <name>`, so the next interpolation this extractor exists to refuse is still
// refused.
const fs = require('fs');
const [,, tsPath, rsPath, modulePath] = process.argv;
const ts = fs.readFileSync(tsPath, 'utf8');
let rs = fs.readFileSync(rsPath, 'utf8');

const ALLOWED_INTERPOLATIONS = ['ESTABLISHED_FACT_TRANSCRIPT_RULE'];

/** Decode one quoted JS string literal starting at `src[i]` (the quote). */
function readStringLiteral(src, i) {
  const q = src[i];
  if (q !== "'" && q !== '"') throw new Error('expected a string literal at ' + i);
  let out = '';
  let j = i + 1;
  while (j < src.length && src[j] !== q) {
    if (src[j] === '\n') throw new Error('newline inside a string literal at ' + j);
    if (src[j] === '\\') {
      const e = src[j + 1];
      const map = { n: '\n', t: '\t', r: '\r', '\\': '\\', "'": "'", '"': '"', '`': '`' };
      if (!(e in map)) throw new Error('unsupported escape \\' + e + ' at ' + j);
      out += map[e];
      j += 2;
    } else {
      out += src[j];
      j += 1;
    }
  }
  if (src[j] !== q) throw new Error('unterminated string literal at ' + i);
  return [out, j + 1];
}

/** `export const NAME = '…' + "…" …` → the concatenated value (literals only). */
function resolveExportedConcat(src, name) {
  const m = src.match(new RegExp('export const ' + name + '\\s*=\\s*'));
  if (!m) throw new Error('missing export ' + name);
  let i = m.index + m[0].length;
  let value = '';
  for (;;) {
    const [lit, next] = readStringLiteral(src, i);
    value += lit;
    i = next;
    const ws = src.slice(i).match(/^[ \t]*(\r?\n[ \t]*)*/)[0];
    if (src[i + ws.length] === '+') {
      i += ws.length + 1;
      i += src.slice(i).match(/^[ \t]*(\r?\n[ \t]*)*/)[0].length;
      continue;
    }
    // The statement must END here: a newline, a `;`, or EOF — anything else
    // (a call, an identifier, a template) is not a literal concatenation.
    const after = src.slice(i).match(/^[ \t]*(;|\r?\n|$)/);
    if (!after) throw new Error('non-literal right-hand side for ' + name);
    return value;
  }
}

const moduleSrc = modulePath ? fs.readFileSync(modulePath, 'utf8') : null;

const names = [...rs.matchAll(/^pub\(crate\) const (\w+): &str = r#"/gm)].map(m => m[1]);
for (const name of names) {
  const m = ts.match(new RegExp('const ' + name + ' = `((?:[^`\\\\]|\\\\.)*)`'));
  if (!m) throw new Error('missing ' + name);
  // Split the body on `${…}`; every placeholder must be an allow-listed name.
  const parts = m[1].split(/\$\{([^}]*)\}/);
  let value = '';
  for (let k = 0; k < parts.length; k++) {
    if (k % 2 === 0) {
      if (parts[k].includes('${')) throw new Error('interpolation in ' + name);
      value += new Function('return `' + parts[k] + '`')();
    } else {
      const ident = parts[k];
      if (!moduleSrc || !ALLOWED_INTERPOLATIONS.includes(ident)) {
        throw new Error('interpolation in ' + name);
      }
      value += resolveExportedConcat(moduleSrc, ident);
    }
  }
  if (value.includes('"#')) throw new Error('raw-string terminator in ' + name);
  const re = new RegExp('(pub\\(crate\\) const ' + name + ': &str = r#")[\\s\\S]*?("#;)');
  rs = rs.replace(re, (_, a, b) => a + value + b);
}
process.stdout.write(rs);
