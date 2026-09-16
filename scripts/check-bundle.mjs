#!/usr/bin/env node

import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const [artifactArgument, frontendArgument = 'dist'] = process.argv.slice(2);
if (!artifactArgument || process.argv.length > 4) {
  throw new Error('usage: npm run check:bundle -- <native-executable> [frontend-directory]');
}
const artifact = resolve(artifactArgument);
const frontend = resolve(frontendArgument);
if (!existsSync(artifact)) throw new Error(`artifact does not exist: ${artifact}`);
if (!existsSync(frontend)) throw new Error(`frontend directory does not exist: ${frontend}`);

const report = {
  artifact,
  platform: process.platform,
  frontendHasNetworkReference: findTextFiles(frontend).some((path) => /https?:\/\//i.test(readFileSync(path, 'utf8'))),
  dependencyInspection: inspectDependencies(artifact),
};
if (report.frontendHasNetworkReference) {
  throw new Error(`frontend bundle contains an http(s) reference; see ${frontend}`);
}
if (process.platform === 'linux' && report.dependencyInspection.output.includes('libarchive.so')) {
  throw new Error('release executable dynamically depends on system libarchive');
}
process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);

function inspectDependencies(path) {
  const command = process.platform === 'darwin' ? ['otool', ['-L', path]] : process.platform === 'win32' ? ['dumpbin', ['/dependents', path]] : ['ldd', [path]];
  const result = spawnSync(command[0], command[1], { encoding: 'utf8' });
  if (result.error) {
    throw new Error(`required ${command[0]} dependency inspector is unavailable: ${result.error.message}`);
  }
  if (result.status !== 0) throw new Error(`${command[0]} failed: ${result.stderr}`);
  return { command: command[0], output: result.stdout };
}

function findTextFiles(path) {
  if (statSync(path).isFile()) return [path];
  return readdirSync(path, { withFileTypes: true }).flatMap((entry) => {
    const child = join(path, entry.name);
    return entry.isDirectory() ? findTextFiles(child) : /\.(?:css|html|js|mjs)$/i.test(basename(child)) ? [child] : [];
  });
}
