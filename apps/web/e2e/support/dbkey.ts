import { createCipheriv, createHash, pbkdf2Sync, randomBytes } from 'node:crypto';

/**
 * Produce a `quilltap.dbkey` file wrapping `pepper` under `passphrase`, byte-
 * compatible with the Rust reader (`crates/quilltap-core/src/dbkey.rs`
 * `save_dbkey`): PBKDF2-HMAC-SHA256 × 600 000 over a 32-byte salt, AES-256-GCM
 * with a 16-byte IV, and the exact JSON field order
 * `{version, algorithm, kdf, kdfIterations, kdfDigest, salt, iv, ciphertext,
 * authTag, pepperHash}` (2-space pretty). A non-empty `passphrase` boots the
 * instance `needs-passphrase` (the locked flow the e2e drives).
 */
export function makeDbKeyFile(pepper: string, passphrase: string): string {
  const salt = randomBytes(32);
  const iv = randomBytes(16);
  const key = pbkdf2Sync(passphrase, salt, 600_000, 32, 'sha256');
  const cipher = createCipheriv('aes-256-gcm', key, iv);
  const ciphertext = Buffer.concat([cipher.update(Buffer.from(pepper, 'utf8')), cipher.final()]);
  const authTag = cipher.getAuthTag();
  const pepperHash = createHash('sha256').update(pepper).digest('hex');
  return (
    JSON.stringify(
      {
        version: 1,
        algorithm: 'aes-256-gcm',
        kdf: 'pbkdf2',
        kdfIterations: 600_000,
        kdfDigest: 'sha256',
        salt: salt.toString('hex'),
        iv: iv.toString('hex'),
        ciphertext: ciphertext.toString('hex'),
        authTag: authTag.toString('hex'),
        pepperHash,
      },
      null,
      2,
    ) + '\n'
  );
}
