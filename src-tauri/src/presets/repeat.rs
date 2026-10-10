//! Per-job variation for repeated exports, with no claim about platform review.
//! The frozen plan contains every seed so a run can be reproduced.
use sha2::{Digest, Sha256};

pub const ID: &str = "repeat-variant-v1";

pub(super) fn video_filter(job_id: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(ID.as_bytes());
    hash.update(b":");
    hash.update(job_id.as_bytes());
    let digest = hash.finalize();
    let seeds: [u32; 3] = std::array::from_fn(|i| {
        let offset = i * 4;
        u32::from_be_bytes([
            digest[offset],
            digest[offset + 1],
            digest[offset + 2],
            digest[offset + 3],
        ]) & 0x7fff_ffff
    });
    // The bundled filter overwrites cN_seed during initialization. Use an
    // explicit all_seed in each plane-only stage so all three seeds take effect.
    format!(
        "format=yuv420p,noise=all_seed={}:c0s=2:c0f=t+u,noise=all_seed={}:c1s=2:c1f=t+u,noise=all_seed={}:c2s=2:c2f=t+u",
        seeds[0], seeds[1], seeds[2]
    )
}
