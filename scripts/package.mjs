import { spawnSync } from 'node:child_process';

const targetByPlatform = {
  linux: 'package:linux',
  win32: 'package:windows',
  darwin: 'package:macos',
};

const target = targetByPlatform[process.platform];
if (!target) {
  throw new Error(`Packaging is not configured for ${process.platform}.`);
}

const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm';
const result = spawnSync(npm, ['run', target], {
  stdio: 'inherit',
  shell: process.platform === 'win32',
});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
