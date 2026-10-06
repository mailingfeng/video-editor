import {readFile, writeFile, mkdir, chmod, lstat} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {resolve, dirname, basename} from 'node:path';
import {fileURLToPath} from 'node:url';
import {unzipSync} from 'fflate';
import {executableArchitecture, hostTarget} from './verify-release.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const digest = data => createHash('sha256').update(data).digest('hex');
async function downloadArchive(url) {
  const response = await fetch(url, {signal: AbortSignal.timeout(120000)});
  if (!response.ok) throw new Error('Tool download failed: ' + response.status);
  return Buffer.from(await response.arrayBuffer());
}
function inspect(bytes, tool, target, name) {
  if (digest(bytes) !== tool.binarySha256) throw new Error('Binary hash mismatch: ' + name);
  const architecture = executableArchitecture(Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength));
  if (architecture !== target) throw new Error(`Binary architecture ${architecture} does not match ${target}`);
  return {architecture, sha256: digest(bytes), sizeBytes: bytes.length};
}
export async function prepareSidecars({target = hostTarget(), manifest,
  cacheDirectory = resolve(root, '.sidecar-cache'), binaryDirectory = resolve(root, 'src-tauri/binaries'),
  download = downloadArchive}) {
  const entry = manifest.targets?.[target];
  if (!entry) throw new Error('No verified tool lock for target: ' + target);
  const result = {status: 'RESOURCES PREPARED', releaseStatus: 'NOT VERIFIED', target, tools: {}};
  await mkdir(cacheDirectory, {recursive: true});
  await mkdir(binaryDirectory, {recursive: true});
  for (const name of ['ffmpeg', 'ffprobe']) {
    const tool = entry.tools?.[name];
    if (!tool?.archiveSha256 || !tool.binarySha256 || !tool.archiveEntry || !tool.url
      || !tool.archiveName || basename(tool.archiveName) !== tool.archiveName) throw new Error('Incomplete tool lock: ' + name);
    const destination = resolve(binaryDirectory, name + '-' + target + (target.includes('windows') ? '.exe' : ''));
    let current;
    try {
      const stat = await lstat(destination);
      if (!stat.isFile() || stat.isSymbolicLink()) throw new Error('Installed resource must be a regular file: ' + name);
      current = await readFile(destination);
    } catch (error) {if (error.code !== 'ENOENT') throw error;}
    if (current) {
      if (digest(current) !== tool.binarySha256) throw new Error('Installed tool hash mismatch: ' + name);
      result.tools[name] = {...inspect(current, tool, target, name), path: destination, existing: true};
      continue;
    }
    const archive = resolve(cacheDirectory, tool.archiveName);
    let bytes;
    try {bytes = await readFile(archive);} catch (error) {
      if (error.code !== 'ENOENT') throw error;
      bytes = await download(tool.url);
      if (digest(bytes) !== tool.archiveSha256) throw new Error('Archive hash mismatch: ' + name);
      await writeFile(archive, bytes, {flag: 'wx'});
    }
    if (digest(bytes) !== tool.archiveSha256) throw new Error('Archive hash mismatch: ' + name);
    // Extract only the locked executable; Windows archives also contain ffplay and documentation.
    const contents = unzipSync(bytes, {filter: file => file.name === tool.archiveEntry});
    const executable = contents[tool.archiveEntry];
    if (!executable) throw new Error('Locked archive entry missing: ' + name);
    const inspected = inspect(executable, tool, target, name);
    await writeFile(destination, executable, {flag: 'wx'});
    await chmod(destination, 0o755);
    result.tools[name] = {...inspected, path: destination, existing: false};
  }
  return result;
}
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    const options = {};
    const args = process.argv.slice(2);
    for (let i = 0; i < args.length; i++) {
      const key = {'--target': 'target', '--archive-dir': 'cacheDirectory'}[args[i]];
      if (!key || !args[i + 1] || args[i + 1].startsWith('--')) throw new Error('Usage: prepare-sidecars.mjs [--target TRIPLE] [--archive-dir PATH]');
      options[key] = args[++i];
    }
    options.manifest = JSON.parse(await readFile(resolve(root, 'tools/sidecars.lock.json'), 'utf8'));
    console.log(JSON.stringify(await prepareSidecars(options), null, 2));
  } catch (error) {console.error(error.message); process.exitCode = 1;}
}
