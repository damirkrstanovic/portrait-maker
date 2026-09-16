#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { deflateSync } from 'node:zlib';
import {
  existsSync,
  mkdirSync,
  realpathSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, relative, resolve } from 'node:path';

const usage = 'usage: npm run benchmark:seed -- <new-temporary-library-path> [count]';
const [destinationArgument, countArgument] = process.argv.slice(2);
if (!destinationArgument || process.argv.length > 4) {
  throw new Error(usage);
}
const count = countArgument === undefined ? 10_000 : Number.parseInt(countArgument, 10);
if (!Number.isInteger(count) || count < 1 || count > 10_000) {
  throw new Error('count must be an integer from 1 through 10000');
}

const temporaryRoot = realpathSync(tmpdir());
const destination = resolve(destinationArgument);
assertInsideTemporaryDirectory(destination, temporaryRoot);
if (existsSync(destination)) {
  throw new Error(`refusing existing library destination: ${destination}`);
}
const sourceRoot = `${destination}-sources`;
if (existsSync(sourceRoot)) {
  throw new Error(`refusing existing synthetic source destination: ${sourceRoot}`);
}

const sourcePaths = createSources(sourceRoot, count);
runCargo(
  ['run', '--release', '--locked', '-p', 'portrait-core', '--bin', 'benchmark-seed', '--', 'seed', destination, ...sourcePaths],
);
const rawMeasurement = runCargo(
  ['run', '--release', '--locked', '-p', 'portrait-core', '--bin', 'benchmark-seed', '--', 'measure', destination],
);
const measurement = JSON.parse(rawMeasurement);
measurement.seed = {
  command: `npm run benchmark:seed -- ${destination} ${count}`,
  count,
  inputSources: sourcePaths,
  generatedAt: new Date().toISOString(),
};
const resultPath = `${destination}.benchmark.json`;
writeFileSync(resultPath, `${JSON.stringify(measurement, null, 2)}\n`, { flag: 'wx' });
process.stdout.write(`${JSON.stringify({ resultPath, ...measurement }, null, 2)}\n`);

function projectRoot() {
  return resolve(dirname(fileURLToPath(import.meta.url)), '..');
}

function runCargo(arguments_) {
  const result = spawnSync('cargo', arguments_, { cwd: projectRoot(), encoding: 'utf8' });
  if (result.stderr) process.stderr.write(result.stderr);
  if (result.stdout) process.stdout.write(result.stdout);
  if (result.status !== 0) {
    throw result.error ?? new Error(`cargo failed with exit status ${result.status}`);
  }
  return result.stdout;
}

function assertInsideTemporaryDirectory(path, root) {
  const relation = relative(root, path);
  if (!relation || relation === '..' || relation.startsWith(`..${process.platform === 'win32' ? '\\' : '/'}`) || isAbsolute(relation)) {
    throw new Error(`benchmark destination must be a child of ${root}`);
  }
}

function createSources(root, count) {
  const sourceCount = 4;
  const images = [
    ['Small.png', 185, 242, [55, 84, 116, 255]],
    ['Medium.png', 330, 432, [78, 109, 143, 255]],
    ['Fulllength.png', 692, 1024, [98, 129, 159, 255]],
  ].map(([name, width, height, color]) => [name, solidPng(width, height, color)]);
  const ancestry = ['elf', 'dwarf', 'human', 'halfling'];
  const archetype = ['archer', 'fighter', 'mage', 'rogue'];
  const sourcePaths = Array.from({ length: sourceCount }, (_, index) => resolve(root, `source-${index + 1}`));
  for (const path of sourcePaths) mkdirSync(path, { recursive: true });

  for (let index = 0; index < count; index += 1) {
    const source = sourcePaths[index % sourceCount];
    const gender = index % 2 === 0 ? 'female' : 'male';
    const folder = `${gender}_${ancestry[index % ancestry.length]}_${archetype[index % archetype.length]}_benchmark_${String(index).padStart(5, '0')}`;
    const portrait = resolve(source, folder);
    mkdirSync(portrait);
    for (const [name, image] of images) writeFileSync(resolve(portrait, name), image, { flag: 'wx' });
  }
  return sourcePaths;
}

function solidPng(width, height, [red, green, blue, alpha]) {
  const raw = Buffer.alloc((width * 4 + 1) * height);
  for (let y = 0; y < height; y += 1) {
    const row = y * (width * 4 + 1);
    for (let x = 0; x < width; x += 1) {
      const pixel = row + 1 + x * 4;
      raw[pixel] = red;
      raw[pixel + 1] = green;
      raw[pixel + 2] = blue;
      raw[pixel + 3] = alpha;
    }
  }
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    pngChunk('IHDR', Buffer.concat([u32(width), u32(height), Buffer.from([8, 6, 0, 0, 0])])),
    pngChunk('IDAT', deflateSync(raw, { level: 9 })),
    pngChunk('IEND', Buffer.alloc(0)),
  ]);
}

function pngChunk(type, data) {
  const kind = Buffer.from(type, 'ascii');
  return Buffer.concat([u32(data.length), kind, data, u32(crc32(Buffer.concat([kind, data])))]);
}

function u32(value) {
  const result = Buffer.alloc(4);
  result.writeUInt32BE(value);
  return result;
}

function crc32(buffer) {
  let crc = 0xffffffff;
  for (const byte of buffer) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
  }
  return (crc ^ 0xffffffff) >>> 0;
}
