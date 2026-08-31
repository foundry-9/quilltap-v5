/**
 * P4.9c DATA-DIRECTORY ORACLE: drives v4's REAL `lib/paths.ts` exports
 * (imported directly, never reimplemented) across an ENVIRONMENT MATRIX, and
 * emits, per case, BOTH the ambient snapshot the functions read AND the
 * resulting `DataDirInfo` — so the Rust port
 * (`services::data_dir::data_dir_info` over `DataDirEnv`) replays the exact
 * inputs and diffs the outputs field-for-field.
 *
 * Why the snapshot rides the row: v4 reads `process.env`, `os.homedir()`,
 * `process.platform`, and two filesystem probes at call time. The Rust side is
 * a pure function over a captured snapshot, so shipping the snapshot with the
 * expectation removes the whole class of "the two sides disagreed about the
 * world, not about the logic" false failures — and means there is no duplicated
 * case list to drift (the differential replays whatever rows it is handed).
 *
 * The `process.platform` cases use `Object.defineProperty` to drive v4's REAL
 * `getPlatform()` down its darwin / linux / win32 arms. NOTE the win32 rows
 * still run on a posix host, so `path.join` yields forward slashes — a real
 * Windows host is a named uncovered divergence (recorded in the Rust module
 * header).
 *
 * Case coverage (20): the platform default (darwin/linux/win32, + win32 with
 * and without APPDATA); the QUILLTAP_DATA_DIR override (plain, `~`-expanded,
 * bare `~`, and the EMPTY-string falsy arm); Docker via each of the three
 * probes (env flag / `/.dockerenv` / `/app`) and Docker ignoring the override;
 * the Electron shell version + the capabilities parse (trim / empty /
 * duplicate); and the QUILLTAP_HOST_DATA_DIR override both inside and outside
 * a container.
 *
 * P4.D134 (v4 `1560bd43b`): `isLimaEnvironment()` and every Lima branch are
 * GONE, and the route no longer answers `isVM`. `LIMA_CONTAINER` therefore
 * survives here as a V4-SIDE-ONLY input: the two `*_lima_flag_inert` cases set
 * it for real and record what v4 now answers, which is the deletion's proof.
 * It is deliberately absent from the emitted `snapshot` — the Rust
 * `DataDirEnv` no longer has a field for it, because v4 reads it nowhere.
 *
 * Run (Node 24, from the v4 checkout — or a pinned worktree when the drift
 * ledger's regen rule requires one):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-data-dir.ndjson \
 *     $N/node --import tsx $V5W/harness/oracle/cases/data-dir-paths.ts
 */

import * as fs from 'node:fs';
import * as os from 'node:os';
import { createRequire } from 'node:module';

/**
 * v4's `lib/paths.ts` does `import fs from 'fs'`, which under tsx's CJS interop
 * is the mutable `require('fs')` module object — NOT the frozen ESM namespace
 * `import * as fs from 'node:fs'` above. The probe patches must land on the
 * former or v4 never sees them (the ESM namespace throws on assignment).
 */
const cjsFs = createRequire(import.meta.url)('fs') as typeof fs;

/** The ambient inputs v4's paths family reads. `null` = the var is UNSET. */
interface EnvMatrix {
  LIMA_CONTAINER?: string;
  DOCKER_CONTAINER?: string;
  QUILLTAP_DATA_DIR?: string;
  QUILLTAP_HOST_DATA_DIR?: string;
  QUILLTAP_SHELL?: string;
  QUILLTAP_SHELL_CAPABILITIES?: string;
  APPDATA?: string;
}

interface CaseSpec {
  name: string;
  env: EnvMatrix;
  /** Override `process.platform` for this case (v4 reads it per call). */
  platform?: 'darwin' | 'linux' | 'win32';
  /** Pretend the two Docker filesystem markers are present. */
  fakeDockerenv?: boolean;
  fakeAppDir?: boolean;
}

const VAR_NAMES = [
  'LIMA_CONTAINER',
  'DOCKER_CONTAINER',
  'QUILLTAP_DATA_DIR',
  'QUILLTAP_HOST_DATA_DIR',
  'QUILLTAP_SHELL',
  'QUILLTAP_SHELL_CAPABILITIES',
  'APPDATA',
] as const;

const CASES: CaseSpec[] = [
  // ── The platform default, per platform ──
  { name: 'default_darwin', env: {}, platform: 'darwin' },
  { name: 'default_linux', env: {}, platform: 'linux' },
  { name: 'default_win32_no_appdata', env: {}, platform: 'win32' },
  {
    name: 'default_win32_with_appdata',
    env: { APPDATA: '/fake/AppData/Roaming' },
    platform: 'win32',
  },

  // ── The QUILLTAP_DATA_DIR override ──
  { name: 'override_absolute', env: { QUILLTAP_DATA_DIR: '/srv/quilltap-data' }, platform: 'darwin' },
  { name: 'override_tilde', env: { QUILLTAP_DATA_DIR: '~/qtdata' }, platform: 'darwin' },
  { name: 'override_bare_tilde', env: { QUILLTAP_DATA_DIR: '~' }, platform: 'darwin' },
  // The falsy arm: an EMPTY string is not an override (`if (envOverride)`).
  { name: 'override_empty_string', env: { QUILLTAP_DATA_DIR: '' }, platform: 'darwin' },
  { name: 'override_relative', env: { QUILLTAP_DATA_DIR: 'relative/data' }, platform: 'linux' },

  // ── Docker, via each of the three probes ──
  { name: 'docker_env_flag', env: { DOCKER_CONTAINER: 'true' }, platform: 'darwin' },
  { name: 'docker_dockerenv_marker', env: {}, platform: 'linux', fakeDockerenv: true },
  { name: 'docker_app_dir_marker', env: {}, platform: 'linux', fakeAppDir: true },
  // The env flag is a STRICT === 'true' compare.
  { name: 'docker_env_flag_not_true', env: { DOCKER_CONTAINER: '1' }, platform: 'darwin' },
  {
    name: 'docker_ignores_override',
    env: { DOCKER_CONTAINER: 'true', QUILLTAP_DATA_DIR: '/somewhere/else' },
    platform: 'linux',
  },

  // ── The retired Lima probe is INERT: the Docker markers now win (v4
  //    `1560bd43b` deleted the Lima-first branch in getPlatform()) ──
  {
    name: 'platform_lima_flag_inert',
    env: { LIMA_CONTAINER: 'true', DOCKER_CONTAINER: 'true' },
    platform: 'darwin',
    fakeDockerenv: true,
    fakeAppDir: true,
  },

  // ── The Electron shell fields + the capabilities parse ──
  {
    name: 'shell_version_and_capabilities',
    env: {
      QUILLTAP_SHELL: '1.4.2',
      QUILLTAP_SHELL_CAPABILITIES: ' OPENS_FS_PATH , DOWNLOADS_FILE,,OPENS_FS_PATH ',
    },
    platform: 'darwin',
  },
  { name: 'shell_empty_string', env: { QUILLTAP_SHELL: '', QUILLTAP_SHELL_CAPABILITIES: '' }, platform: 'darwin' },

  // ── The host-path override, inside and outside a container ──
  {
    name: 'host_path_outside_container',
    env: { QUILLTAP_HOST_DATA_DIR: '/host/side/Quilltap' },
    platform: 'darwin',
  },
  {
    name: 'host_path_inside_docker',
    env: { DOCKER_CONTAINER: 'true', QUILLTAP_HOST_DATA_DIR: '/host/side/Quilltap' },
    platform: 'darwin',
  },
  // getHostDataDir() no longer honours LIMA_CONTAINER — the override is
  // ignored outside a container and the base dir wins.
  {
    name: 'host_path_lima_flag_inert',
    env: { LIMA_CONTAINER: 'true', QUILLTAP_HOST_DATA_DIR: '/host/side/Quilltap' },
    platform: 'linux',
  },
];

/**
 * Patch the two filesystem probes v4's `isDockerEnvironment` uses. This is the
 * ONLY way to exercise the marker arms on a developer machine; the probes are
 * `fs.existsSync('/.dockerenv')` and `fs.statSync('/app').isDirectory()`.
 * Everything else about the function still runs for real.
 */
function patchFsProbes(c: CaseSpec): () => void {
  const realExists = cjsFs.existsSync;
  const realStat = cjsFs.statSync;
  (cjsFs as { existsSync: typeof fs.existsSync }).existsSync = ((p: fs.PathLike) =>
    p === '/.dockerenv' ? Boolean(c.fakeDockerenv) : realExists(p)) as typeof fs.existsSync;
  (cjsFs as { statSync: typeof fs.statSync }).statSync = ((p: fs.PathLike, ...rest: unknown[]) => {
    if (p === '/app') {
      if (!c.fakeAppDir) throw new Error('ENOENT: no such file or directory, stat /app');
      return { isDirectory: () => true } as fs.Stats;
    }
    return (realStat as (...a: unknown[]) => unknown)(p, ...rest);
  }) as typeof fs.statSync;
  return () => {
    (cjsFs as { existsSync: typeof fs.existsSync }).existsSync = realExists;
    (cjsFs as { statSync: typeof fs.statSync }).statSync = realStat;
  };
}

function applyEnv(env: EnvMatrix): void {
  for (const name of VAR_NAMES) {
    const v = (env as Record<string, string | undefined>)[name];
    if (v === undefined) delete process.env[name];
    else process.env[name] = v;
  }
}

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  // The REAL v4 module — every expectation below is its output, not a re-derivation.
  const paths = await import('@/lib/paths');

  const realPlatform = process.platform;
  const outLines: string[] = [];

  for (const c of CASES) {
    applyEnv(c.env);
    const restoreFs = patchFsProbes(c);
    if (c.platform) {
      Object.defineProperty(process, 'platform', { value: c.platform, configurable: true });
    }

    try {
      const dirInfo = paths.getBaseDataDirWithSource();
      const platform = paths.getPlatform();
      const isDocker = paths.isDockerEnvironment();

      // The snapshot the Rust `DataDirEnv` is built from — the ambient world as
      // these calls saw it.
      const snapshot = {
        dockerContainer: process.env.DOCKER_CONTAINER ?? null,
        quilltapDataDir: process.env.QUILLTAP_DATA_DIR ?? null,
        quilltapHostDataDir: process.env.QUILLTAP_HOST_DATA_DIR ?? null,
        quilltapShell: process.env.QUILLTAP_SHELL ?? null,
        quilltapShellCapabilities: process.env.QUILLTAP_SHELL_CAPABILITIES ?? null,
        appdata: process.env.APPDATA ?? null,
        homeDir: os.homedir(),
        osPlatform: process.platform,
        dockerenvExists: Boolean(c.fakeDockerenv),
        appIsDir: Boolean(c.fakeAppDir),
      };

      // The route's response object, in the route's own field order.
      const info = {
        path: dirInfo.path,
        source: dirInfo.source,
        sourceDescription: dirInfo.sourceDescription,
        platform,
        isDocker,
        isElectronShell: paths.isElectronShell(),
        shellVersion: paths.getElectronShellVersion(),
        shellCapabilities: [...paths.getShellCapabilities()],
        canOpen: !isDocker,
        hostPath: paths.getHostDataDir(),
      };

      outLines.push(JSON.stringify({ name: c.name, snapshot, info }));
    } finally {
      restoreFs();
      if (c.platform) {
        Object.defineProperty(process, 'platform', { value: realPlatform, configurable: true });
      }
    }
  }

  applyEnv({});
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`data-dir oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

main().catch((err) => {
  process.stderr.write(`data-dir oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
