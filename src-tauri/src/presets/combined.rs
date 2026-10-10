//! Bounded per-job media changes; platform originality outcomes are unverified.
//! Every choice is written into the frozen processing plan, with no frame edits.
use sha2::{Digest, Sha256};

pub const ID: &str = "repeat-combined-v2";

pub(super) struct Parameters {
    digest: [u8; 32],
    pub crf: u8,
    pub gop: u16,
}

impl Parameters {
    pub fn for_job(job_id: &str) -> Self {
        let mut hash = Sha256::new();
        hash.update(ID.as_bytes());
        hash.update(b":");
        hash.update(job_id.as_bytes());
        let digest: [u8; 32] = hash.finalize().into();
        Self {
            crf: 22 + digest[28] % 2,
            gop: 180 + u16::from(digest[29]) % 141,
            digest,
        }
    }

    fn fraction(&self, index: usize) -> f64 {
        f64::from(self.digest[index]) / 255.0
    }

    pub fn video_filter(&self, width: u32, height: u32) -> String {
        let sign = if self.digest[12] & 1 == 0 { 1.0 } else { -1.0 };
        let mut offset = sign * (0.008 + 0.006 * self.fraction(13));
        let mut swing = 0.003 + 0.004 * self.fraction(14);
        // Limit padding for narrow inputs. Reserve the whole rotated rectangle
        // instead of cropping source edges, then resize to the original size.
        let bound = (offset.abs() + swing)
            .min(0.018 * f64::from(width.min(height)) / f64::from(width.max(height)));
        let factor = bound / (offset.abs() + swing);
        offset *= factor;
        swing *= factor;
        let canvas_w =
            (((f64::from(width) + f64::from(height) * bound.sin()) / 2.0).ceil() * 2.0) as u64;
        let canvas_h =
            (((f64::from(height) + f64::from(width) * bound.sin()) / 2.0).ceil() * 2.0) as u64;
        // Scale preserves the expanded canvas DAR by modifying SAR. Undo that
        // adjustment so anamorphic inputs retain their original pixel aspect.
        let sar_restore = f64::from(width) / f64::from(height) * canvas_h as f64 / canvas_w as f64;
        let phase = std::f64::consts::TAU * self.fraction(15);
        let period = 18.0 + 11.0 * self.fraction(16);
        let contrast = 0.96 + 0.08 * self.fraction(17);
        let brightness = -0.01 + 0.02 * self.fraction(18);
        let gamma = 0.97 + 0.06 * self.fraction(19);
        let saturation = 0.94 + 0.12 * self.fraction(20);
        let detail = 0.2 + 0.25 * self.fraction(21);
        let noise = 4 + self.digest[22] % 3;
        let seeds: [u32; 3] = std::array::from_fn(|i| {
            let at = i * 4;
            u32::from_be_bytes(self.digest[at..at + 4].try_into().unwrap()) & 0x7fff_ffff
        });
        format!(
            "format=yuv420p,rotate=angle='{offset:.6}+{swing:.6}*sin(2*PI*if(isnan(t),0,t)/{period:.6}+{phase:.6})':ow={canvas_w}:oh={canvas_h}:c=black,scale={width}:{height}:flags=lanczos,setsar=sar*{sar_restore:.12}:max=65535,eq=contrast={contrast:.6}:brightness='{brightness:.6}+0.008*sin(2*PI*if(isnan(t),0,t)/{period:.6}+{phase:.6})':gamma={gamma:.6}:saturation={saturation:.6}:eval=frame,unsharp=3:3:{detail:.6}:3:3:0,noise=all_seed={}:c0s={noise}:c0f=t+u,noise=all_seed={}:c1s={noise}:c1f=t+u,noise=all_seed={}:c2s={noise}:c2f=t+u",
            seeds[0], seeds[1], seeds[2]
        )
    }

    pub fn audio_filter(&self) -> String {
        let phase_frequency = 350 + u32::from(self.digest[23]) * 3;
        let highpass = 60 + self.digest[24] % 50;
        let lowpass = 11000 + u32::from(self.digest[25]) * 16;
        let phase = std::f64::consts::TAU * self.fraction(26);
        let period = 7.0 + 8.0 * self.fraction(27);
        let threshold = 0.08 + 0.08 * self.fraction(30);
        let ratio = 1.3 + 0.6 * self.fraction(31);
        // Resample before frequency filters, process channels without mixing,
        // and compensate limiter lookahead including the stream's final tail.
        format!(
            "aresample=48000,highpass=f={highpass},lowpass=f={lowpass},allpass=f={phase_frequency}:t=q:w=0.8,acompressor=threshold={threshold:.6}:ratio={ratio:.6}:attack=10:release=100:makeup=1:link=average:detection=rms,volume='0.94+0.035*sin(2*PI*if(isnan(t),0,t)/{period:.6}+{phase:.6})':eval=frame,alimiter=limit=0.95:level=0:latency=1"
        )
    }
}
