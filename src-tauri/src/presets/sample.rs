//! An empirical approximation of the sample-1 media changes, not the original
//! software's recovered command or evidence of platform review outcomes.
//! Calibration and limitations: docs/requirements/20261002-research/sample-1-calibration.md.

pub const ID: &str = "sample-match-v1";
pub const V2_ID: &str = "sample-match-v2";
pub const VIDEO_FILTER: &str = "lutyuv=y='clip(0.985*val+0.5,0,255)'";

// The measured spectral envelope is approximated independently on each channel.
// Timestamp compensation avoids introducing the FIR's delay into A/V alignment.
pub const AUDIO_FILTER: &str = concat!(
    "firequalizer=gain_entry='",
    "entry(0,-60);entry(50,-29.5);entry(75,-20.8);entry(100,-14);",
    "entry(150,-6.1);entry(200,-6.7);entry(300,-3.4);entry(450,-1.7);",
    "entry(700,-2.4);entry(1000,-3.5);entry(1500,-1.5);entry(2000,-1.5);",
    "entry(3000,0.6);entry(4000,-1.4);entry(5000,-3.3);entry(6500,-7.3);",
    "entry(8000,-13.6);entry(10000,-14.8);entry(12000,-2.5);",
    "entry(16000,-2.5);entry(24000,-3.5)':delay=0.007:zero_phase=on"
);

// Pair-1 and pair-2 transfer measurements favor a causal bandpass with about
// 5 ms of content delay. Keep v1 frozen for A/B comparison; apply delay to every
// channel, including surround inputs. Both delivered v2 outputs were reported
// to have failed platform review; waveform matching does not establish success.
pub const V2_AUDIO_FILTER: &str = "highpass=f=300,lowpass=f=5500,adelay=5:all=1,volume=0.8";
