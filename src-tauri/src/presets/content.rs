//! Visible reframing and audible pitch changes; platform outcomes remain unverified.
use crate::contracts::AudioInfo;
use sha2::{Digest, Sha256};

pub const ID: &str = "content-variation-v1";

pub(super) struct Parameters {
    digest: [u8; 32],
    pub crf: u8,
    pub gop: u16,
}

impl Parameters {
    pub fn for_job(job_id: &str) -> Self {
        let digest: [u8; 32] = Sha256::digest(format!("{ID}:{job_id}")).into();
        Self {
            crf: 20 + digest[28] % 3,
            gop: 180 + u16::from(digest[29]) % 141,
            digest,
        }
    }

    fn fraction(&self, index: usize) -> f64 {
        f64::from(self.digest[index]) / 255.0
    }

    pub fn video_filter(&self, width: u32, height: u32) -> String {
        let scale = 0.80 + 0.06 * self.fraction(12);
        let foreground_w = ((f64::from(width) * scale / 2.0).floor() * 2.0).max(2.0) as u32;
        let foreground_h = ((f64::from(height) * scale / 2.0).floor() * 2.0).max(2.0) as u32;
        let move_x = (f64::from(width) * 0.015).min(f64::from(width - foreground_w) / 4.0);
        let move_y = (f64::from(height) * 0.015).min(f64::from(height - foreground_h) / 4.0);
        let blur = (f64::from(width.min(height)) * 0.03).clamp(3.0, 28.0);
        let period = 8.0 + 6.0 * self.fraction(13);
        let phase = std::f64::consts::TAU * self.fraction(14);
        let hue = -8.0 + 16.0 * self.fraction(15);
        let contrast = 1.03 + 0.07 * self.fraction(16);
        let brightness = -0.02 + 0.04 * self.fraction(17);
        let saturation = 1.02 + 0.10 * self.fraction(18);
        let noise = 4 + self.digest[19] % 3;
        let seed = u32::from_be_bytes(self.digest[0..4].try_into().unwrap()) & 0x7fff_ffff;
        // Both branches keep every timestamp; the complete frame is resized,
        // never cropped. The original-size background carries the input SAR.
        format!(
            "format=yuv420p,split=2[bg][fg];[bg]gblur=sigma={blur:.3}:steps=2,eq=brightness=-0.14:saturation=0.35[back];[fg]scale={foreground_w}:{foreground_h}:flags=lanczos,eq=contrast={contrast:.6}:brightness={brightness:.6}:saturation={saturation:.6},hue=h='{hue:.6}+4*sin(2*PI*if(isnan(t),0,t)/{period:.6}+{phase:.6})',noise=all_seed={seed}:alls={noise}:allf=t+u[front];[back][front]overlay=x='(W-w)/2+{move_x:.6}*sin(2*PI*if(isnan(t),0,t)/{period:.6}+{phase:.6})':y='(H-h)/2+{move_y:.6}*cos(2*PI*if(isnan(t),0,t)/{period:.6}+{phase:.6})':shortest=1:repeatlast=0,format=yuv420p"
        )
    }

    pub fn audio_filter(&self, audio: &AudioInfo) -> String {
        let sign = if self.digest[20] & 1 == 0 { 1.0 } else { -1.0 };
        let semitones = sign * (2.0 + self.fraction(21));
        let pitch_rate = (48000.0 * 2.0_f64.powf(semitones / 12.0)).round() as u32;
        let tempo = 48000.0 / f64::from(pitch_rate);
        let duration =
            audio.duration_ticks as f64 * audio.time_base.num as f64 / audio.time_base.den as f64;
        let samples = (duration * 48000.0).round() as u64;
        let start =
            audio.start_pts as f64 * audio.time_base.num as f64 / audio.time_base.den as f64;
        let period = 7.0 + 5.0 * self.fraction(22);
        let phase = std::f64::consts::TAU * self.fraction(23);
        let lowpass = 10000 + u32::from(self.digest[24]) * 16;
        let shift = format!("asetrate={pitch_rate},aresample=48000,atempo={tempo:.12},apad=whole_len={samples},atrim=end_sample={samples}");
        let mut filter = "aresample=48000,asetpts=PTS-STARTPTS,".to_owned();
        if audio.channels == 1 {
            filter.push_str(&shift);
        } else {
            // Independent WSOLA searches avoid high-frequency channels driving
            // the low-frequency channel's overlap alignment. Pure mappings
            // extract and reassemble indices without mixing or swapping them.
            filter.push_str(&format!("asplit={}", audio.channels));
            for channel in 0..audio.channels {
                filter.push_str(&format!("[channel{channel}]"));
            }
            for channel in 0..audio.channels {
                filter.push_str(&format!(
                    ";[channel{channel}]pan=mono|c0=c{channel},{shift},aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=mono[shifted{channel}]"
                ));
            }
            filter.push(';');
            for channel in 0..audio.channels {
                filter.push_str(&format!("[shifted{channel}]"));
            }
            let layout = audio
                .channel_layout
                .clone()
                .unwrap_or_else(|| format!("{}c", audio.channels));
            filter.push_str(&format!("amerge=inputs={},pan={layout}", audio.channels));
            for channel in 0..audio.channels {
                filter.push_str(&format!("|c{channel}=c{channel}"));
            }
        }
        // Bound each channel's EOF rounding, then restore the recorded start.
        filter.push_str(&format!(",asetpts=PTS+{start:.9}/TB,highpass=f=45,lowpass=f={lowpass},acompressor=threshold=0.18:ratio=1.6:attack=10:release=100:makeup=1:link=average:detection=rms,volume='0.90+0.03*sin(2*PI*if(isnan(t),0,t)/{period:.6}+{phase:.6})':eval=frame,alimiter=limit=0.95:level=0:latency=1"));
        filter
    }
}
