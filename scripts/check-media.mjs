import { spawnSync } from 'node:child_process';
import { access } from 'node:fs/promises';
import path from 'node:path';
import {generateFixtures, root} from './generate-fixtures.mjs';
const args = process.argv.slice(2);
if (args.length !== 0 && !(args.length === 2 && args[0] === '--sample-dir')) throw new Error('Usage: npm run test:media -- --sample-dir PATH');
const fixtureDir = await generateFixtures();
let sampleDir = args[1] ? path.resolve(args[1]) : null;
if (sampleDir) {
  // The research document and original media live at different levels in this repo.
  // Locate by presence, then pass an explicit read-only source directory to Rust.
  for (let i = 0; i < 5; i++) {
    try {await access(path.join(sampleDir, '原视频.mp4')); break;}
    catch {if (i === 4 || path.dirname(sampleDir) === sampleDir) throw new Error('原视频.mp4 not found above --sample-dir'); sampleDir = path.dirname(sampleDir);}
  }
}
const env = {...process.env,VIDEO_EDITOR_FIXTURE_DIR:fixtureDir,...(sampleDir ? {VIDEO_EDITOR_SAMPLE_DIR:sampleDir} : {})};
const r = spawnSync('cargo', ['test','--locked','--manifest-path','src-tauri/Cargo.toml','--test','pipeline_real',...(sampleDir ? [] : ['generated_media_end_to_end']),'--','--ignored','--test-threads=1'], {
  cwd:root, stdio:'inherit', env,
});
if (r.error) throw r.error;
process.exitCode = r.status ?? 1;
if (process.exitCode === 0) {
  const lifecycle = spawnSync('cargo', ['test','--locked','--manifest-path','src-tauri/Cargo.toml','--test','lifecycle_real','generated_','--','--ignored','--test-threads=1'], {cwd:root, stdio:'inherit', env});
  if (lifecycle.error) throw lifecycle.error;
  process.exitCode = lifecycle.status ?? 1;
}
