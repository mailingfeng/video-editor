import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp, mkdir, writeFile, rm} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {verifyPackage, executableArchitecture} from './verify-release.mjs';
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const isWindows = process.platform === 'win32';
const target = isWindows ? 'x86_64-pc-windows-msvc' : 'x86_64-apple-darwin';
async function fixture(t) {
  const dir = await mkdtemp(path.join(tmpdir(),'frameshift-release-'));
  t.after(() => rm(dir,{recursive:true,force:true}));
  const artifact = path.join(dir,isWindows ? 'setup.exe' : '帧序.app');
  const bin = isWindows ? path.join(dir,'installed') : path.join(artifact,'Contents','MacOS');
  const resources = isWindows ? bin : path.join(artifact,'Contents','Resources');
  await mkdir(bin,{recursive:true}); await mkdir(resources,{recursive:true});
  const bytes = Buffer.alloc(isWindows ? 128 : 32);
  if (isWindows) {
    bytes.write('MZ'); bytes.writeUInt32LE(64,0x3c); bytes.write('PE\0\0',64); bytes.writeUInt16LE(0x8664,68);
    await writeFile(artifact,bytes);
  } else {
    bytes.writeUInt32LE(0xfeedfacf,0); bytes.writeUInt32LE(0x01000007,4);
  }
  const file = name => path.join(bin,isWindows ? `${name}.exe` : name);
  for (const name of ['video-editor','ffmpeg','ffprobe']) await writeFile(file(name),bytes,{mode:0o755});
  const manifest = {schemaVersion:1,targets:{[target]:{version:'9.0.2-test',distributionStatus:'ready',tools:Object.fromEntries(['ffmpeg','ffprobe'].map(name=>[name,{binarySha256:hash(bytes),version:`${name} version 9.0.2-test`,buildconf:'--enable-libx264',license:'GNU General Public License version 3'}])),materials:[]}}};
  for (const [kind,file] of [['license','GPL.txt'],['corresponding-source','source.tar.xz']]) {
    const content = Buffer.from(`fixture ${kind}`); await writeFile(path.join(resources,file),content);
    manifest.targets[target].materials.push({kind,path:file,sha256:hash(content)});
  }
  const options = {target,artifact,manifest,development:true};
  if (isWindows) options.installedDirectory = bin;
  // Native process probing is a boundary double; file content/hash/architecture checks are real.
  const native = {hostTarget:target,run(file,args) {
    if (args.includes('-version')) return `${path.basename(file,'.exe')} version 9.0.2-test`;
    if (args.includes('-buildconf')) return '--enable-libx264';
    if (args.includes('-L')) return 'GNU General Public License version 3';
    return '';
  }};
  return {dir,bin,resources,options,native,bytes,file};
}
test('wrong_architecture_is_rejected',async t=> {
  const f = await fixture(t); const arm = Buffer.from(f.bytes);
  if (isWindows) arm.writeUInt16LE(0xaa64,68); else arm.writeUInt32LE(0x0100000c,4);
  await writeFile(f.file('ffmpeg'),arm);
  const r = await verifyPackage(f.options,f.native);
  assert.equal(r.exitCode,1); assert.match(r.errors.join('\n'),/architecture/);
});
test('missing_sidecar_is_rejected',async t=> {
  const f = await fixture(t); await rm(f.file('ffprobe'));
  const r = await verifyPackage(f.options,f.native);
  assert.equal(r.exitCode,1); assert.match(r.errors.join('\n'),/ffprobe.*missing/);
});
test('missing_license_material_is_rejected',async t=> {
  const f = await fixture(t); f.options.development=false;
  await rm(path.join(f.resources,'GPL.txt'));
  const r = await verifyPackage(f.options,f.native);
  assert.equal(r.exitCode,1); assert.match(r.errors.join('\n'),/license.*missing/);
});
test('valid_development_resources_return_zero_without_release_claim',async t=> {
  const f = await fixture(t); const r = await verifyPackage(f.options,f.native);
  assert.equal(r.exitCode,0,r.errors.join('\n')); assert.equal(r.status,'DEVELOPMENT CHECKED');
  assert.equal(r.releaseStatus,'NOT VERIFIED'); assert.equal(Object.keys(r.tools).length,2);
});
test('tampered_resource_hash_is_rejected',async t=> {
  const f = await fixture(t); await writeFile(f.file('ffprobe'),Buffer.concat([f.bytes,Buffer.from('tampered')]));
  const r = await verifyPackage(f.options,f.native);
  assert.equal(r.exitCode,1); assert.match(r.errors.join('\n'),/checksum/);
});
test('foreign_host_does_not_count_as_native_verification',async t=> {
  const f = await fixture(t); f.native.hostTarget='aarch64-apple-darwin';
  const r = await verifyPackage(f.options,f.native);
  assert.equal(r.exitCode,1); assert.match(r.errors.join('\n'),/native host/);
});
test('reads_windows_PE_machine_and_rejects_truncated_headers',()=> {
  const pe = Buffer.alloc(128); pe.write('MZ');pe.writeUInt32LE(64,0x3c);pe.write('PE\0\0',64);pe.writeUInt16LE(0x8664,68);
  assert.equal(executableArchitecture(pe),'x86_64-pc-windows-msvc');
  assert.throws(()=>executableArchitecture(Buffer.from('MZ')),/header/);
});

async function windowsFixture(t) {
  const f = await fixture(t);
  const target = 'x86_64-pc-windows-msvc';
  const installedDirectory = path.join(f.dir,'中文 installed');
  await mkdir(installedDirectory);
  const pe = Buffer.alloc(128); pe.write('MZ'); pe.writeUInt32LE(64,0x3c);
  pe.write('PE\0\0',64); pe.writeUInt16LE(0x8664,68);
  for (const name of ['video-editor','ffmpeg','ffprobe']) await writeFile(path.join(installedDirectory,`${name}.exe`),pe);
  const artifact = path.join(f.dir,'setup.exe'); await writeFile(artifact,pe);
  const manifest = {targets:{[target]:{distributionStatus:'development-only',materials:[],tools:
    Object.fromEntries(['ffmpeg','ffprobe'].map(name=>[name,{binarySha256:hash(pe),versionToken:'9.0.2-essentials_build-www.gyan.dev'}]))}}};
  const native = {hostTarget:target,run(file,args) {
    if (args.includes('-version')) return `${path.basename(file,'.exe')} version 9.0.2-essentials_build-www.gyan.dev Copyright (c) developers\r\nmore output\r\n`;
    if (args.includes('-buildconf')) return '--enable-libx264';
    if (args.includes('-L')) return 'GNU General Public License version 3';
    return 'NotSigned';
  }};
  return {options:{target,artifact,installedDirectory,manifest,development:true},native};
}
test('windows_CRLF_version_output_matches_the_complete_locked_token',async t=> {
  const f = await windowsFixture(t);
  const r = await verifyPackage(f.options,f.native);
  assert.equal(r.exitCode,0,r.errors.join('\n'));
  assert.equal(r.status,'DEVELOPMENT CHECKED'); assert.equal(r.releaseStatus,'NOT VERIFIED');
  assert.equal(Object.keys(r.tools).length,2); assert.ok(!r.tools.ffmpeg.version.endsWith('\r'));
});
test('windows_version_token_does_not_accept_a_prefix_match',async t=> {
  const f = await windowsFixture(t);
  f.options.manifest.targets[f.options.target].tools.ffmpeg.versionToken='9.0.2';
  const r = await verifyPackage(f.options,f.native);
  assert.equal(r.exitCode,1); assert.match(r.errors.join('\n'),/ffmpeg: version mismatch/);
});
test('windows_installer_without_installed_directory_is_not_verified',async t=> {
  const f = await windowsFixture(t); delete f.options.installedDirectory;
  const r = await verifyPackage(f.options,f.native);
  assert.equal(r.exitCode,1); assert.match(r.errors.join('\n'),/installed directory missing/);
});
