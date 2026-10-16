use super::FrozenCustomParameters;
use crate::{
    contracts::*,
    media::timing::audio_timing,
    presets::opaque::{hidden, render},
};

impl FrozenCustomParameters {
    pub fn audio_filter(
        &self,
        audio: &AudioInfo,
        video: &VideoInfo,
    ) -> Result<Option<String>, AppError> {
        if !self.has_audio_effects() && self.settings.speed == 0 {
            return Ok(None);
        }
        let (start, _, samples) = audio_timing(audio, video, &self.timing())?;
        let speed = self.speed();
        let mut pitch_factor = 1.0;
        if self.settings.pitch > 0 {
            let sign = if self.random[9][0] & 1 == 0 {
                1.0
            } else {
                -1.0
            };
            let semitones = sign * Self::weight(self.settings.pitch) * (2.0 + self.fraction(9, 1));
            let rate = (48000.0 * 2.0_f64.powf(semitones / 12.0)).round() as u32;
            pitch_factor = f64::from(rate) / 48000.0;
        }
        let channel_filters = if self.settings.pitch > 0 || self.settings.speed > 0 {
            vec![
                render!("rubberband=tempo={tempo}:pitch={pitch}:channels=apart:pitchq=quality:window=long:transients=smooth", "{tempo}"=>format!("{:.15}",speed.num as f64/speed.den as f64), "{pitch}"=>format!("{pitch_factor:.15}")),
            ]
        } else {
            Vec::new()
        };
        let mut filter = hidden!("aresample=48000,asetpts=PTS-STARTPTS");
        if !channel_filters.is_empty() {
            let shift = channel_filters.join(",");
            if audio.channels == 1 {
                filter.push(',');
                filter.push_str(&shift);
            } else {
                filter.push_str(
                    &render!(",asplit={channels}","{channels}"=>audio.channels.to_string()),
                );
                for channel in 0..audio.channels {
                    filter.push_str(&format!("[channel{channel}]"));
                }
                for channel in 0..audio.channels {
                    filter.push_str(&render!(";[channel{channel}]pan=mono|c0=c{channel},{shift},aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=mono[shifted{channel}]",
                        "{channel}"=>channel.to_string(),"{shift}"=>shift.clone()));
                }
                filter.push(';');
                for channel in 0..audio.channels {
                    filter.push_str(&format!("[shifted{channel}]"));
                }
                let layout = audio
                    .channel_layout
                    .clone()
                    .unwrap_or_else(|| format!("{}c", audio.channels));
                filter.push_str(&render!("amerge=inputs={channels},pan={layout}","{channels}"=>audio.channels.to_string(),"{layout}"=>layout));
                for channel in 0..audio.channels {
                    filter.push_str(&format!("|c{channel}=c{channel}"));
                }
            }
        }
        if self.settings.timbre > 0 {
            let weight = Self::weight(self.settings.timbre);
            let highpass = 45.0 * weight;
            let lowpass =
                24000.0 * ((10000.0 + 4080.0 * self.fraction(10, 0)) / 24000.0).powf(weight);
            let ratio = 1.0 + 0.6 * weight;
            filter.push_str(&render!(",highpass=f={hp},lowpass=f={lp},acompressor=threshold=0.18:ratio={ratio}:attack=10:release=100:makeup=1:link=average:detection=rms",
                "{hp}"=>format!("{highpass:.9}"),"{lp}"=>format!("{lowpass:.9}"),"{ratio}"=>format!("{ratio:.9}")));
        }
        filter.push_str(&render!(",asetpts=PTS+{start}/TB","{start}"=>format!("{start:.15}")));
        if self.settings.volume > 0 {
            let weight = Self::weight(self.settings.volume);
            filter.push_str(&render!(",volume='{mean}+{amplitude}*sin(2*PI*if(isnan(t),0,t)/{period}+{phase})':eval=frame",
                "{mean}"=>format!("{:.9}",1.0-0.10*weight),"{amplitude}"=>format!("{:.9}",0.03*weight),"{period}"=>format!("{:.9}",7.0+5.0*self.fraction(11,0)),"{phase}"=>format!("{:.9}",std::f64::consts::TAU*self.fraction(11,1))));
        }
        if self.has_audio_effects() {
            filter.push_str(&hidden!(",alimiter=limit=0.95:level=0:latency=1"));
        }
        filter.push_str(&render!(",apad=pad_len=1024,atrim=end_sample={samples}","{samples}"=>samples.to_string()));
        Ok(Some(filter))
    }
}
