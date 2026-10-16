#[cfg(test)]
mod tests {
    use super::*;
    fn video(width: u32, height: u32) -> VideoInfo {
        serde_json::from_value(serde_json::json!({"streamIndex":0,"codec":"h264","width":width,"height":height,"bitDepth":8,"pixelFormat":"yuv420p","frameRate":{"num":30,"den":1},"timeBase":{"num":1,"den":15360},"frameCount":60,"startPts":0,"durationTicks":30720,"bitRate":null,"colorRange":null,"colorSpace":null,"colorPrimaries":null,"colorTransfer":null})).unwrap()
    }
    #[test]
    fn recommended_geometry_is_milder_than_level_five() {
        let settings = crate::contracts::CustomVariationSettings::recommended();
        let mild = FrozenCustomParameters::resolve(&settings, "geometry").unwrap();
        let mut stronger = settings.clone();
        stronger.foreground_scale = 5;
        stronger.background_blur = 5;
        let strong = FrozenCustomParameters::resolve(&stronger, "geometry").unwrap();
        let v = video(1280, 720);
        let a = mild.geometry(&v).unwrap();
        let b = strong.geometry(&v).unwrap();
        assert!((0.92..=0.944).contains(&a.ratio));
        assert!(a.foreground_w > b.foreground_w && a.foreground_h > b.foreground_h);
        assert!((a.sigma / b.sigma - 0.4).abs() < 1e-12);
        assert_eq!(a.crop_w, 1280);
        assert_eq!(a.crop_h, 720);
    }
    #[test]
    fn all_levels_keep_crop_and_overlay_inside_even_frame() {
        for level in 0..=9 {
            let mut settings = crate::contracts::CustomVariationSettings::recommended();
            settings.foreground_scale = level;
            settings.background_blur = level;
            settings.dynamic_offset = level;
            settings.crop_scale = level;
            let p = FrozenCustomParameters::resolve(&settings, "geometry").unwrap();
            for (w, h) in [(2, 2), (2, 128), (128, 2), (720, 1280), (1280, 720)] {
                let g = p.geometry(&video(w, h)).unwrap();
                assert!(g.foreground_w >= 2 && g.foreground_w <= w && g.foreground_w % 2 == 0);
                assert!(g.foreground_h >= 2 && g.foreground_h <= h && g.foreground_h % 2 == 0);
                assert!(g.crop_w >= 2 && g.crop_w <= w && g.crop_w % 2 == 0);
                assert!(g.crop_h >= 2 && g.crop_h <= h && g.crop_h % 2 == 0);
                assert!(g.move_x <= f64::from(w - g.foreground_w) / 4.0);
                assert!(g.move_y <= f64::from(h - g.foreground_h) / 4.0);
            }
        }
    }
}

use super::FrozenCustomParameters;
use crate::presets::opaque::{hidden, render};
use crate::{
    contracts::*,
    media::timing::{project_video_pts, video_target},
};

struct Geometry {
    #[cfg(test)]
    ratio: f64,
    foreground_w: u32,
    foreground_h: u32,
    crop_w: u32,
    crop_h: u32,
    move_x: f64,
    move_y: f64,
    sigma: f64,
}
impl FrozenCustomParameters {
    fn geometry(&self, video: &VideoInfo) -> Result<Geometry, AppError> {
        let (w, h) = (video.width, video.height);
        if w < 2 || h < 2 || w % 2 != 0 || h % 2 != 0 {
            return Err(crate::media::timing::timing_error());
        }
        let weight = Self::weight(self.settings.foreground_scale);
        let ratio = 1.0 - weight * (1.0 - (0.80 + 0.06 * self.fraction(0, 0)));
        let even = |dimension: u32, factor: f64| {
            (((f64::from(dimension) * factor / 2.0).floor() * 2.0).max(2.0) as u32).min(dimension)
        };
        let foreground_w = even(w, ratio);
        let foreground_h = even(h, ratio);
        let crop = |dimension: u32, index: usize| {
            let removed =
                f64::from(self.settings.crop_scale) / 9.0 * (0.01 + 0.03 * self.fraction(3, index));
            even(dimension, 1.0 - removed)
        };
        let movement = Self::weight(self.settings.dynamic_offset);
        Ok(Geometry {
            #[cfg(test)]
            ratio,
            foreground_w,
            foreground_h,
            crop_w: crop(w, 0),
            crop_h: crop(h, 1),
            move_x: (f64::from(w) * 0.015 * movement).min(f64::from(w - foreground_w) / 4.0),
            move_y: (f64::from(h) * 0.015 * movement).min(f64::from(h - foreground_h) / 4.0),
            sigma: Self::weight(self.settings.background_blur)
                * (f64::from(w.min(h)) * 0.03).clamp(3.0, 28.0),
        })
    }
    fn picture_filter(&self, time: &str) -> Vec<String> {
        let mut filters = Vec::new();
        if self.settings.saturation > 0 || self.settings.tone > 0 {
            let contrast =
                1.0 + Self::weight(self.settings.tone) * (0.03 + 0.07 * self.fraction(6, 0));
            let brightness =
                Self::weight(self.settings.tone) * (-0.02 + 0.04 * self.fraction(6, 1));
            let saturation =
                1.0 + Self::weight(self.settings.saturation) * (0.02 + 0.10 * self.fraction(5, 0));
            filters.push(render!("eq=contrast={c}:brightness={b}:saturation={s}","{c}"=>format!("{contrast:.9}"),"{b}"=>format!("{brightness:.9}"),"{s}"=>format!("{saturation:.9}")));
        }
        if self.settings.hue > 0 {
            let weight = Self::weight(self.settings.hue);
            filters.push(render!("hue=h='{base}+{amplitude}*sin(2*PI*({time})/{period}+{phase})'",
                "{base}"=>format!("{:.9}",weight*(-8.0+16.0*self.fraction(4,0))),
                "{amplitude}"=>format!("{:.9}",4.0*weight),"{time}"=>time.into(),
                "{period}"=>format!("{:.9}",8.0+6.0*self.fraction(4,1)),"{phase}"=>format!("{:.9}",std::f64::consts::TAU*self.fraction(4,2))));
        }
        if self.settings.noise > 0 {
            let strength = (Self::weight(self.settings.noise)
                * f64::from(4 + self.random[7][0] % 3))
            .round() as u8;
            let seed = u32::from_be_bytes(self.random[7][1..5].try_into().unwrap()) & 0x7fff_ffff;
            filters.push(render!("noise=all_seed={seed}:alls={strength}:allf=t+u","{seed}"=>seed.to_string(),"{strength}"=>strength.to_string()));
        }
        if self.settings.softness > 0 {
            let sigma = Self::weight(self.settings.softness) * (0.2 + 0.3 * self.fraction(8, 0));
            filters.push(render!("gblur=sigma={sigma}:steps=2","{sigma}"=>format!("{sigma:.9}")));
        }
        filters
    }
    pub fn video_filter(&self, video: &VideoInfo) -> Result<Option<String>, AppError> {
        let g = self.geometry(video)?;
        let (w, h) = (video.width, video.height);
        let speed = self.speed();
        let origin =
            video.start_pts as f64 * video.time_base.num as f64 / video.time_base.den as f64;
        // Effects run before setpts, so express periods in final playback time.
        let time = render!("({origin}+(if(isnan(t),0,t)-{origin})*{q}/{p})","{origin}"=>format!("{origin:.12}"),"{q}"=>speed.den.to_string(),"{p}"=>speed.num.to_string());
        let picture = self.picture_filter(&time);
        let layout = self.settings.foreground_scale > 0;
        let crop = self.settings.crop_scale > 0;
        if !layout && !crop && picture.is_empty() && self.settings.speed == 0 {
            return Ok(None);
        }
        let mut prefix = hidden!("format=yuv420p");
        if crop {
            let x = ((w - g.crop_w) / 2) / 2 * 2;
            let y = ((h - g.crop_h) / 2) / 2 * 2;
            prefix.push_str(&render!(",crop={cw}:{ch}:{x}:{y},scale={w}:{h}:flags=lanczos,setsar=sar*{w}*{ch}/({h}*{cw})",
                "{cw}"=>g.crop_w.to_string(),"{ch}"=>g.crop_h.to_string(),"{x}"=>x.to_string(),"{y}"=>y.to_string(),"{w}"=>w.to_string(),"{h}"=>h.to_string()));
        }
        if layout {
            let mut background = Vec::new();
            if self.settings.background_blur > 0 {
                background.push(
                    render!("gblur=sigma={sigma}:steps=2","{sigma}"=>format!("{:.9}",g.sigma)),
                );
            }
            let weight = Self::weight(self.settings.foreground_scale);
            background.push(render!("eq=brightness={b}:saturation={s}","{b}"=>format!("{:.9}",-0.14*weight),"{s}"=>format!("{:.9}",(1.0-0.65*weight).max(0.0))));
            let mut front = vec![
                render!("scale={w}:{h}:flags=lanczos","{w}"=>g.foreground_w.to_string(),"{h}"=>g.foreground_h.to_string()),
            ];
            front.extend(picture);
            let period = 8.0 + 6.0 * self.fraction(2, 0);
            let phase = std::f64::consts::TAU * self.fraction(2, 1);
            prefix.push_str(&render!(",split=2[bg][fg];[bg]{background}[back];[fg]{front}[front];[back][front]overlay=x='(W-w)/2+{x}*sin(2*PI*({time})/{period}+{phase})':y='(H-h)/2+{y}*cos(2*PI*({time})/{period}+{phase})':shortest=1:repeatlast=0,format=yuv420p",
                "{background}"=>background.join(","),"{front}"=>front.join(","),"{x}"=>format!("{:.9}",g.move_x),"{y}"=>format!("{:.9}",g.move_y),"{time}"=>time,"{period}"=>format!("{period:.9}"),"{phase}"=>format!("{phase:.9}")));
        } else if !picture.is_empty() {
            prefix.push(',');
            prefix.push_str(&picture.join(","));
        }
        if self.settings.speed > 0 {
            let timing = self.timing();
            let (base, _, _, _) = video_target(video, &timing)?;
            let start = project_video_pts(video, &timing, video.start_pts)?;
            prefix.push_str(&render!(",settb=expr={num}/{den},setpts='(PTS-{start})/{p}*{q}+{start}'",
                "{num}"=>base.num.to_string(),"{den}"=>base.den.to_string(),
                "{p}"=>speed.num.to_string(),"{q}"=>speed.den.to_string(),"{start}"=>start.to_string()));
        }
        Ok(Some(prefix))
    }
}
