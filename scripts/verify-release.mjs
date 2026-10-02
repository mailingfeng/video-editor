import {spawnSync} from 'node:child_process';
import {readFile, open, lstat, writeFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {release as osRelease} from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const supported = ['x86_64-apple-darwin','aarch64-apple-darwin','x86_64-pc-windows-msvc'];
const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');
export function hostTarget() {
  if (process.platform === 'darwin') return `${process.arch === 'arm64' ? 'aarch64' : 'x86_64'}-apple-darwin`;
  return process.platform === 'win32' && process.arch === 'x64' ? 'x86_64-pc-windows-msvc' : 'unsupported';
}
export function executableArchitecture(bytes) {
  if (bytes.length < 8) throw new Error('truncated executable header');
  if (bytes.readUInt32LE(0) === 0xfeedfacf) {
    const cpu = bytes.readUInt32LE(4);
    if (cpu === 0x01000007) return 'x86_64-apple-darwin';
    if (cpu === 0x0100000c) return 'aarch64-apple-darwin';
  }
  if (bytes.subarray(0,2).toString() === 'MZ') {
    if (bytes.length < 64) throw new Error('truncated PE header');
    const offset = bytes.readUInt32LE(0x3c);
    if (offset > bytes.length - 6 || bytes.subarray(offset,offset+4).toString() !== 'PE\0\0') throw new Error('invalid PE header');
    if (bytes.readUInt16LE(offset+4) === 0x8664) return 'x86_64-pc-windows-msvc';
  }
  throw new Error('unsupported executable architecture/header');
}
function runNative(file,args) {
  const env = {...process.env,PATH:process.platform === 'win32' ? process.env.PATH : '/usr/bin:/bin'};
  const r = spawnSync(file,args,{encoding:'utf8',timeout:30000,maxBuffer:2*1024*1024,env});
  if (r.error || r.status !== 0) throw new Error(r.error?.message || `${file}: exit ${r.status}: ${r.stderr.slice(-1000)}`);
  return r.stdout + r.stderr;
}
async function regular(file) {
  const stat = await lstat(file);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size === 0) throw new Error('missing or non-regular file');
  return stat;
}
async function inspect(file,target,expectedHash) {
  const stat = await regular(file);
  const handle = await open(file,'r');
  let header;
  try {header = Buffer.alloc(Math.min(stat.size,65536)); await handle.read(header,0,header.length,0);}
  finally {await handle.close();}
  const architecture = executableArchitecture(header);
  if (architecture !== target) throw new Error(`architecture ${architecture} does not match ${target}`);
  if (!target.includes('windows') && (stat.mode & 0o111) === 0) throw new Error('executable permission missing');
  const digest = sha(await readFile(file));
  if (expectedHash && digest !== expectedHash) throw new Error('checksum mismatch');
  return {architecture,sha256:digest,sizeBytes:stat.size};
}
export async function verifyPackage(options, boundary = {}) {
  const {target,artifact,manifest,development=false,installedDirectory,receiptPath} = options;
  const run = boundary.run ?? runNative;
  const host = boundary.hostTarget ?? hostTarget();
  const result = {exitCode:1,status:'NOT VERIFIED',releaseStatus:'NOT VERIFIED',target,host,osRelease:osRelease(),verifiedAt:new Date().toISOString(),artifact:path.resolve(artifact),tools:{},main:null,fingerprint:null,errors:[],warnings:[],signatures:{},installation:null};
  if (!supported.includes(target)) result.errors.push('unsupported target');
  if (host !== target) result.errors.push(`native host ${host} does not match ${target}`);
  const entry = manifest.targets?.[target];
  if (!entry) result.errors.push('locked tool resources missing for target');
  if (result.errors.length) return result;
  const windows = target.includes('windows');
  const packageDir = windows ? installedDirectory : artifact;
  if (!packageDir) {result.errors.push('Windows installed directory missing; an installer alone is not native verification');return result;}
  const bin = windows ? packageDir : path.join(packageDir,'Contents','MacOS');
  const resources = windows ? packageDir : path.join(packageDir,'Contents','Resources');
  const main = path.join(bin,windows ? 'video-editor.exe' : 'video-editor');
  try {result.main = await inspect(main,target);} catch(e) {result.errors.push(`main: ${e.message}`);}
  if (windows) {try {await regular(artifact);} catch(e) {result.errors.push(`installer: ${e.message}`);}}
  for (const name of ['ffmpeg','ffprobe']) {
    const file = path.join(bin,`${name}${windows ? '.exe' : ''}`);
    try {
      const locked = entry.tools?.[name];
      if (!locked?.binarySha256 || (!locked.version && !locked.versionToken)) throw new Error('locked resource missing');
      const tool = await inspect(file,target,locked.bundledBinarySha256 ?? locked.binarySha256);
      tool.version = run(file,['-version']).split('\n')[0].trimEnd();
      const parts = tool.version.split(/\s+/);
      const matches = locked.version ? tool.version === locked.version
        : parts[0] === name && parts[1] === 'version' && parts[2] === locked.versionToken;
      if (!matches) throw new Error('version mismatch');
      tool.buildconf = run(file,['-buildconf']);
      if (!tool.buildconf.includes('--enable-libx264')) throw new Error('libx264 build capability missing');
      tool.license = run(file,['-L']);
      result.tools[name] = tool;
    } catch(e) {result.errors.push(`${name}: ${e.code === 'ENOENT' ? 'missing' : e.message}`);}
  }
  const materials = entry.materials ?? [];
  const materialErrors = [];
  for (const kind of ['license','corresponding-source']) {
    if (!materials.some(m=>m.kind===kind)) materialErrors.push(`${kind} material missing from lock manifest`);
  }
  for (const m of materials) {
    try {
      const resolved = path.resolve(resources,m.path);
      if (!resolved.startsWith(path.resolve(resources)+path.sep)) throw new Error('material path escapes resources');
      await regular(resolved);
      if (!m.sha256 || sha(await readFile(resolved)) !== m.sha256) throw new Error('checksum mismatch');
    } catch(e) {materialErrors.push(`${m.kind} material ${m.path}: ${e.code === 'ENOENT' ? 'missing' : e.message}`);}
  }
  if (entry.distributionStatus !== 'ready') materialErrors.push(`resource distribution status: ${entry.distributionStatus}`);
  (development ? result.warnings : result.errors).push(...materialErrors);
  if (result.main && Object.keys(result.tools).length === 2) {
    result.fingerprint = sha(Buffer.from(JSON.stringify([target,result.main.sha256,result.tools.ffmpeg.sha256,result.tools.ffprobe.sha256])));
  }
  const signatureErrors = [];
  for (const file of [main,...['ffmpeg','ffprobe'].map(n=>path.join(bin,n+(windows?'.exe':''))),...(windows?[artifact]:[])]) {
    try {
      if (windows) {
        const literal = file.replaceAll("'","''");
        // Load the module from this PowerShell's installation, bypassing inherited PS7 paths.
        const status = run('powershell.exe',['-NoProfile','-NonInteractive','-Command',`Import-Module ($PSHOME + '\\Modules\\Microsoft.PowerShell.Security\\Microsoft.PowerShell.Security.psd1') -ErrorAction Stop; (Get-AuthenticodeSignature -LiteralPath '${literal}').Status.ToString()`]).trim();
        if (status !== 'Valid') throw new Error(`Authenticode ${status}`);
        result.signatures[path.basename(file)] = status;
      } else {
        run('/usr/bin/codesign',['--verify','--strict',file]);
        const info = run('/usr/bin/codesign',['-d','--verbose=4',file]);
        if (!info.includes('Authority=Developer ID Application:')) throw new Error('Developer ID signature missing');
        result.signatures[path.basename(file)] = info;
      }
    } catch(e) {signatureErrors.push(`${path.basename(file)} signature: ${e.message}`);}
  }
  if (!windows) {
    try {run('/usr/bin/codesign',['--verify','--deep','--strict',artifact]);run('/usr/sbin/spctl',['--assess','--type','execute','--verbose=2',artifact]);run('/usr/bin/xcrun',['stapler','validate',artifact]);}
    catch(e) {signatureErrors.push(`bundle assessment/notarization: ${e.message}`);}
  }
  (development ? result.warnings : result.errors).push(...signatureErrors);
  if (receiptPath) {
    try {
      const receipt = JSON.parse(await readFile(receiptPath,'utf8'));
      const cases = ['installedLaunch','noDevelopmentTools','conversion','cancel','unicodeAndSpaces','outputConflict','startupRecovery'];
      if (receipt.target !== target || receipt.fingerprint !== result.fingerprint || !receipt.os || !receipt.cpu || !receipt.testedAt || cases.some(c=>receipt.cases?.[c]!==true)) throw new Error('matching native installation evidence incomplete');
      result.installation = receipt;
    } catch(e) {result.errors.push(`installation receipt: ${e.message}`);}
  } else (development ? result.warnings : result.errors).push('native installation receipt missing');
  if (!result.errors.length) {result.exitCode=0;result.status=development?'DEVELOPMENT CHECKED':'VERIFIED';if(!development)result.releaseStatus='VERIFIED';}
  return result;
}
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2); const options = {};
  const keys = {'--target':'target','--artifact':'artifact','--installed-dir':'installedDirectory','--receipt':'receiptPath','--record':'record','--manifest':'manifestPath'};
  try {
    for(let i=0;i<args.length;i++) {
      if(args[i]==='--development') {options.development=true;continue;}
      const key = keys[args[i]]; if(!key || !args[i+1] || args[i+1].startsWith('--')) throw new Error('invalid arguments');
      options[key]=args[++i];
    }
    if(!options.target || !options.artifact) throw new Error('Usage: node scripts/verify-release.mjs --target TRIPLE --artifact PATH [--development] [--installed-dir PATH] [--receipt PATH] [--record PATH]');
    options.manifest=JSON.parse(await readFile(options.manifestPath ?? new URL('../tools/sidecars.lock.json',import.meta.url),'utf8'));
    const result = await verifyPackage(options);
    const json = JSON.stringify(result,null,2)+'\n';
    if(options.record) await writeFile(options.record,json,{flag:'wx'});
    console.log(json); process.exitCode=result.exitCode;
  } catch(e) {console.error(e.message);process.exitCode=1;}
}
