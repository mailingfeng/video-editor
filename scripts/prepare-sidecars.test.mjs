import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp, mkdir, readFile, writeFile, rm, symlink} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {zipSync} from 'fflate';
import {prepareSidecars} from './prepare-sidecars.mjs';

const target = 'x86_64-pc-windows-msvc';
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
function executable(machine = 0x8664) {
  const bytes = Buffer.alloc(128);
  bytes.write('MZ'); bytes.writeUInt32LE(64, 0x3c);
  bytes.write('PE\0\0', 64); bytes.writeUInt16LE(machine, 68);
  return bytes;
}
async function fixture(t, bytes = executable()) {
  const root = await mkdtemp(path.join(tmpdir(), 'frameshift-sidecars-'));
  t.after(() => rm(root, {recursive: true, force: true}));
  const entries = Object.fromEntries(['ffmpeg', 'ffprobe'].map(name => [`release/bin/${name}.exe`, bytes]));
  const archive = Buffer.from(zipSync(entries));
  const tools = Object.fromEntries(['ffmpeg', 'ffprobe'].map(name => [name, {
    url: 'https://example.test/release.zip', archiveName: 'release.zip',
    archiveEntry: `release/bin/${name}.exe`, archiveSha256: sha(archive), binarySha256: sha(bytes),
  }]));
  const options = {target, manifest: {targets: {[target]: {version: 'test', tools}}},
    cacheDirectory: path.join(root, 'cache'), binaryDirectory: path.join(root, 'binaries')};
  let downloads = 0;
  options.download = async () => {downloads++; return archive;};
  return {options, archive, bytes, downloads: () => downloads,
    destination: name => path.join(options.binaryDirectory, `${name}-${target}.exe`)};
}
test('prepares_nested_windows_executables_from_one_locked_archive', async t => {
  const f = await fixture(t); const result = await prepareSidecars(f.options);
  assert.equal(f.downloads(), 1);
  assert.equal(result.status, 'RESOURCES PREPARED');
  assert.equal(result.releaseStatus, 'NOT VERIFIED');
  for (const name of ['ffmpeg', 'ffprobe']) assert.deepEqual(await readFile(f.destination(name)), f.bytes);
});
test('matching_hash_does_not_allow_wrong_executable_architecture', async t => {
  const f = await fixture(t, executable(0xaa64));
  await assert.rejects(prepareSidecars(f.options), /architecture/);
  await assert.rejects(readFile(f.destination('ffmpeg')), {code: 'ENOENT'});
});
test('tampered_cached_archive_never_publishes_an_executable', async t => {
  const f = await fixture(t); await mkdir(f.options.cacheDirectory);
  await writeFile(path.join(f.options.cacheDirectory, 'release.zip'), Buffer.from('tampered'));
  await assert.rejects(prepareSidecars(f.options), /Archive hash mismatch/);
  assert.equal(f.downloads(), 0);
  await assert.rejects(readFile(f.destination('ffmpeg')), {code: 'ENOENT'});
});
test('verified_existing_resources_are_idempotent_and_work_offline', async t => {
  const f = await fixture(t); await prepareSidecars(f.options);
  f.options.download = async () => {throw Error('network unavailable');};
  const result = await prepareSidecars(f.options);
  assert.equal(Object.keys(result.tools).length, 2);
  assert.equal(result.tools.ffmpeg.existing, true);
});
test('changed_existing_resource_is_rejected_without_replacement', async t => {
  const f = await fixture(t); await prepareSidecars(f.options);
  await writeFile(f.destination('ffmpeg'), Buffer.from('changed'));
  await assert.rejects(prepareSidecars(f.options), /Installed tool hash mismatch/);
  assert.equal((await readFile(f.destination('ffmpeg'))).toString(), 'changed');
});
test('existing_resource_symlink_is_rejected', async t => {
  const f = await fixture(t); await mkdir(f.options.binaryDirectory);
  const outside = path.join(f.options.cacheDirectory, 'outside.exe');
  await mkdir(f.options.cacheDirectory); await writeFile(outside, f.bytes);
  // Windows directory junctions avoid requiring Developer Mode/admin for a file symlink.
  await symlink(process.platform === 'win32' ? f.options.cacheDirectory : outside,
    f.destination('ffmpeg'), process.platform === 'win32' ? 'junction' : 'file');
  await assert.rejects(prepareSidecars(f.options), /regular file/);
});
test('unlocked_target_does_not_download_or_create_resources', async t => {
  const f = await fixture(t); f.options.target = 'aarch64-pc-windows-msvc';
  await assert.rejects(prepareSidecars(f.options), /tool lock/);
  assert.equal(f.downloads(), 0);
});
