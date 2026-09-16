import { spawnSync } from 'node:child_process';

const args = process.argv.slice(2);
const result = process.platform === 'linux'
  ? spawnSync('bash', ['scripts/package-linux.sh', '--dev', ...args], { stdio: 'inherit' })
  : spawnSync(process.execPath, ['node_modules/@tauri-apps/cli/tauri.js', 'dev', ...args], { stdio: 'inherit' });
if (result.error) throw result.error;
process.exit(result.status ?? 1);
