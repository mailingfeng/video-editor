import { spawnSync } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
export const root = fileURLToPath(new URL('../', import.meta.url));
export const fixtureDir = path.join(root, '.media-fixtures');
export const target = process.platform === 'darwin' ? `${process.arch === 'arm64' ? 'aarch64' : 'x86_64'}-apple-darwin` : 'x86_64-pc-windows-msvc';
export async function generateFixtures() {
  await mkdir(fixtureDir, {recursive:true});
  const ffmpeg = path.join(root, 'src-tauri', 'binaries', `ffmpeg-${target}${process.platform === 'win32' ? '.exe' : ''}`);
  const make = (name, args) => {
    const r = spawnSync(ffmpeg, ['-v','error','-y',...args,path.join(fixtureDir,name)], {encoding:'utf8'});
    if (r.error || r.status !== 0) throw new Error(`${name}: ${r.error?.message || r.stderr}`);
  };
  const video = ['-f','lavfi','-i','testsrc2=size=128x96:rate=30'];
  const audio = ['-f','lavfi','-i','sine=frequency=440:sample_rate=44100'];
  const encode = ['-t','1.2','-c:v','libx264','-pix_fmt','yuv420p','-c:a','aac'];
  make("短片 空格 '引号'.mp4", [...video,...audio,...encode]);
  make('no-audio.mp4', [...video,...encode]);
  make('lifecycle 空格.mp4', [...video,...audio,'-t','10','-c:v','libx264','-pix_fmt','yuv420p','-c:a','aac']);
  make('full-range.mp4', [...video,'-t','1.2','-c:v','mjpeg','-pix_fmt','yuvj420p','-color_range','pc','-colorspace','bt709']);
  make('rgb.mp4', ['-f','lavfi','-i','testsrc=size=128x96:rate=30','-t','1.2','-c:v','libx264rgb','-pix_fmt','rgb24']);
  make('av-offset.mp4', [...video,'-itsoffset','0.12',...audio,...encode]);
  make('vfr.mp4', [...video,'-vf',"select='not(eq(mod(n,7),0))'",'-fps_mode','vfr',...encode]);
  make('hdr.mp4', [...video,'-t','1.2','-c:v','libx264','-pix_fmt','yuv420p10le','-color_primaries','bt2020','-color_trc','smpte2084','-colorspace','bt2020nc']);
  await writeFile(path.join(fixtureDir,'damaged.mp4'), Buffer.from('incomplete MP4 header'));
  return fixtureDir;
}
if (process.argv[1] === fileURLToPath(import.meta.url)) console.log(await generateFixtures());
