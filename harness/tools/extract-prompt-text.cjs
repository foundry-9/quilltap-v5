// Mechanical regen of crates/quilltap-core/src/services/context_summary/prompt_text.rs:
// evaluate each named template literal from v4's chat-tasks.ts and splice its
// runtime value into the existing file's r#"…"# bodies (header + order kept).
const fs = require('fs');
const [,, tsPath, rsPath] = process.argv;
const ts = fs.readFileSync(tsPath, 'utf8');
let rs = fs.readFileSync(rsPath, 'utf8');
const names = [...rs.matchAll(/^pub\(crate\) const (\w+): &str = r#"/gm)].map(m => m[1]);
for (const name of names) {
  const m = ts.match(new RegExp('const ' + name + ' = `((?:[^`\\\\]|\\\\.)*)`'));
  if (!m) throw new Error('missing ' + name);
  if (m[1].includes('${')) throw new Error('interpolation in ' + name);
  const value = new Function('return `' + m[1] + '`')();
  if (value.includes('"#')) throw new Error('raw-string terminator in ' + name);
  const re = new RegExp('(pub\\(crate\\) const ' + name + ': &str = r#")[\\s\\S]*?("#;)');
  rs = rs.replace(re, (_, a, b) => a + value + b);
}
process.stdout.write(rs);
