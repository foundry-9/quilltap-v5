/**
 * @jest-environment node
 *
 * P4.D63 ARCHIVE-CRYPTO ORACLE (tier 1, exact) — drives v4's REAL
 * `lib/characters/archive-crypto.ts` (v4 `d553f72a`) with `crypto.randomBytes`
 * mocked so the salt and IV are pinned. That makes `encryptArchive` a pure
 * function of (plaintext, passphrase, salt, iv), so the Rust port's output can
 * be compared BYTE FOR BYTE rather than merely "decrypts to the same thing".
 *
 * What it emits, per case:
 *   - `encrypt` — the full bundle, hex, from v4's real `encryptArchive`, plus
 *     the parsed header fields (so a mismatch says WHICH field moved) and the
 *     round-trip proof that v4 can read its own bytes back.
 *   - `decrypt_error` — the error `name` + `message` v4 throws for every
 *     refusal arm (wrong passphrase / corrupt tag / bad magic / truncation).
 *     The Rust side compares BOTH, which is what makes the four typed errors'
 *     sentences contractual.
 *   - `is_encrypted` — the magic probe over assorted byte strings.
 *
 * The 600k-iteration PBKDF2 is deliberately NOT reduced: it is part of the
 * bundle format, and a reduced-iteration corpus would prove nothing about the
 * bundles a real instance writes. Each case pays it twice (encrypt + decrypt),
 * hence the raised timeout.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-archive-crypto-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/archive-crypto.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-archive-crypto.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- archive-crypto
 */

import * as fs from 'fs';

/** Deterministic filler so a salt/IV is recognizable in a diff. */
const fill = (byte: number, len: number): Buffer => Buffer.alloc(len, byte);

/** The pinned randomBytes stream: salt (32) then IV (16), per encrypt call. */
let randomQueue: Buffer[] = [];

interface EncryptCase {
  name: string;
  kind: 'encrypt';
  plaintext: string;
  passphrase: string;
  saltByte: number;
  ivByte: number;
}
interface DecryptErrorCase {
  name: string;
  kind: 'decrypt_error';
  /** How to build the bytes handed to decryptArchive. */
  build: 'wrong-passphrase' | 'corrupt-tag' | 'bad-magic' | 'truncated-length' | 'truncated-header' | 'header-not-json';
  passphrase: string;
}
interface IsEncryptedCase {
  name: string;
  kind: 'is_encrypted';
  /** Hex of the probe bytes; `''` = the empty buffer. */
  hex: string;
}
type Case = EncryptCase | DecryptErrorCase | IsEncryptedCase;

const CASES: Case[] = [
  // Byte-exactness across plaintext shapes: empty, ASCII, multibyte UTF-8, and
  // a size that crosses no chunk boundary but is large enough that a
  // chunking-dependent implementation would differ if one existed.
  { name: 'encrypt_empty', kind: 'encrypt', plaintext: '', passphrase: 'correct horse', saltByte: 0x07, ivByte: 0x09 },
  { name: 'encrypt_ascii', kind: 'encrypt', plaintext: '{"kind":"character","name":"Fenn"}\n', passphrase: 'correct horse', saltByte: 0x07, ivByte: 0x09 },
  { name: 'encrypt_utf8', kind: 'encrypt', plaintext: 'Fenn — the Retired Cartographer ✒️ «bearings»\n', passphrase: 'correct horse', saltByte: 0x11, ivByte: 0x22 },
  { name: 'encrypt_large', kind: 'encrypt', plaintext: 'x'.repeat(200_000), passphrase: 'correct horse', saltByte: 0x33, ivByte: 0x44 },
  // The internal sentinel is a real passphrase to this layer.
  { name: 'encrypt_internal_sentinel', kind: 'encrypt', plaintext: 'no-passphrase instance', passphrase: '__quilltap_no_passphrase__', saltByte: 0x55, ivByte: 0x66 },
  // A passphrase with non-ASCII bytes — PBKDF2 keys off the UTF-8 encoding.
  { name: 'encrypt_utf8_passphrase', kind: 'encrypt', plaintext: 'guarded', passphrase: 'pässwörd — ✒️', saltByte: 0x77, ivByte: 0x88 },

  { name: 'decrypt_wrong_passphrase', kind: 'decrypt_error', build: 'wrong-passphrase', passphrase: 'the new one' },
  { name: 'decrypt_corrupt_tag', kind: 'decrypt_error', build: 'corrupt-tag', passphrase: 'correct horse' },
  { name: 'decrypt_bad_magic', kind: 'decrypt_error', build: 'bad-magic', passphrase: 'correct horse' },
  { name: 'decrypt_truncated_length', kind: 'decrypt_error', build: 'truncated-length', passphrase: 'correct horse' },
  { name: 'decrypt_truncated_header', kind: 'decrypt_error', build: 'truncated-header', passphrase: 'correct horse' },
  { name: 'decrypt_header_not_json', kind: 'decrypt_error', build: 'header-not-json', passphrase: 'correct horse' },

  { name: 'is_encrypted_true', kind: 'is_encrypted', hex: Buffer.from('QTAPARC1 and more', 'ascii').toString('hex') },
  { name: 'is_encrypted_exact_magic', kind: 'is_encrypted', hex: Buffer.from('QTAPARC1', 'ascii').toString('hex') },
  { name: 'is_encrypted_short', kind: 'is_encrypted', hex: Buffer.from('QTAPARC', 'ascii').toString('hex') },
  { name: 'is_encrypted_other', kind: 'is_encrypted', hex: Buffer.from('PKzipfile', 'binary').toString('hex') },
  { name: 'is_encrypted_empty', kind: 'is_encrypted', hex: '' },
];

describe('archive-crypto oracle', () => {
  it('emits the archive-crypto corpus', async () => {
    const outPath = process.env.QT_ORACLE_OUT;
    if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

    // Pin randomBytes for the salt/IV draws; delegate anything else to the real
    // implementation so no other consumer is starved. NO `__esModule: true`
    // and no explicit `default` — see the `jest-crypto-randombytes-mock` note:
    // adding either makes `import crypto from 'crypto'` resolve to undefined.
    jest.resetModules();
    jest.doMock('crypto', () => {
      const actual = jest.requireActual('crypto');
      return {
        ...actual,
        randomBytes: (n: number) => {
          const next = randomQueue.shift();
          if (next && next.length === n) return next;
          if (next) throw new Error(`randomBytes(${n}) but the queue held ${next.length}`);
          return actual.randomBytes(n);
        },
      };
    });

    const mod = await import('@/lib/characters/archive-crypto');
    const { encryptArchive, decryptArchive, isEncryptedArchive } = mod;

    /** Encrypt with a pinned salt+IV. */
    const seal = (plaintext: Buffer, passphrase: string, saltByte: number, ivByte: number): Buffer => {
      randomQueue = [fill(saltByte, 32), fill(ivByte, 16)];
      const out = encryptArchive(plaintext, passphrase);
      randomQueue = [];
      return out;
    };

    const out = fs.createWriteStream(outPath, { flags: 'w' });
    const write = (row: unknown) => out.write(`${JSON.stringify(row)}\n`);

    for (const c of CASES) {
      if (c.kind === 'encrypt') {
        const plaintext = Buffer.from(c.plaintext, 'utf8');
        const bundle = seal(plaintext, c.passphrase, c.saltByte, c.ivByte);
        // Parse the header back out so a diff names the field that moved.
        const headerLength = bundle.readUInt32BE(8);
        const header = JSON.parse(bundle.subarray(12, 12 + headerLength).toString('utf8'));
        // v4 reads its own bytes back — the round trip both sides must satisfy.
        const roundTrip = decryptArchive(bundle, c.passphrase);
        write({
          name: c.name,
          kind: c.kind,
          input: {
            plaintextHex: plaintext.toString('hex'),
            passphrase: c.passphrase,
            saltHex: fill(c.saltByte, 32).toString('hex'),
            ivHex: fill(c.ivByte, 16).toString('hex'),
          },
          bundleHex: bundle.toString('hex'),
          headerJson: bundle.subarray(12, 12 + headerLength).toString('utf8'),
          header,
          roundTripHex: roundTrip.toString('hex'),
        });
        continue;
      }

      if (c.kind === 'decrypt_error') {
        // Every arm starts from one good bundle and damages it a specific way,
        // so the corpus records what the SAME bytes do differently.
        const good = seal(Buffer.from('secret cargo', 'utf8'), 'correct horse', 0x07, 0x09);
        let bytes: Buffer;
        switch (c.build) {
          case 'wrong-passphrase':
            bytes = good;
            break;
          case 'corrupt-tag': {
            bytes = Buffer.from(good);
            bytes[bytes.length - 1] ^= 0xff;
            break;
          }
          case 'bad-magic':
            bytes = Buffer.from('not an archive at all', 'ascii');
            break;
          case 'truncated-length':
            bytes = Buffer.from('QTAPARC1', 'ascii');
            break;
          case 'truncated-header':
            bytes = good.subarray(0, 12 + 4);
            break;
          case 'header-not-json': {
            // A well-formed frame whose header bytes are not JSON at all.
            const junk = Buffer.from('this is not json at all!', 'utf8');
            const len = Buffer.alloc(4);
            len.writeUInt32BE(junk.length);
            bytes = Buffer.concat([
              new Uint8Array(Buffer.from('QTAPARC1', 'ascii')),
              new Uint8Array(len),
              new Uint8Array(junk),
              new Uint8Array(Buffer.alloc(32)), // body + a full-size tag
            ]);
            break;
          }
        }
        let name = '<no throw>';
        let message = '<no throw>';
        try {
          decryptArchive(bytes, c.passphrase);
        } catch (err) {
          name = err instanceof Error ? err.name : 'unknown';
          message = err instanceof Error ? err.message : String(err);
        }
        write({ name: c.name, kind: c.kind, bytesHex: bytes.toString('hex'), passphrase: c.passphrase, error: { name, message } });
        continue;
      }

      write({
        name: c.name,
        kind: c.kind,
        hex: c.hex,
        result: isEncryptedArchive(Buffer.from(c.hex, 'hex')),
      });
    }

    await new Promise<void>((resolve) => out.end(resolve));
    expect(true).toBe(true);
  }, 300_000);
});
