//! Lightweight encoding of processing strings, not a secrecy boundary.
const fn seed() -> u64 {
    let bytes = env!("FRAMESHIFT_TEMPLATE_SEED").as_bytes();
    let mut result = 0u64;
    let mut index = 0;
    while index < bytes.len() {
        result = result
            .wrapping_mul(10)
            .wrapping_add((bytes[index] - b'0') as u64);
        index += 1;
    }
    result
}
const KEY: u64 = seed();
const fn mask(index: usize, key: u64) -> u8 {
    (key.rotate_left((index % 64) as u32) ^ (index as u64).wrapping_mul(0x9e3779b97f4a7c15)) as u8
}
pub(crate) const fn encode<const N: usize>(text: &[u8]) -> [u8; N] {
    let mut bytes = [0; N];
    let mut index = 0;
    while index < N {
        bytes[index] = text[index] ^ mask(index, KEY);
        index += 1;
    }
    bytes
}
#[inline(never)]
pub(crate) fn decode(encoded: &[u8]) -> String {
    let encoded = std::hint::black_box(encoded);
    let key = std::hint::black_box(KEY);
    let bytes = encoded
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ mask(index, key))
        .collect();
    String::from_utf8(bytes).expect("invalid internal text")
}
pub(crate) fn substitute(mut text: String, values: &[(&str, String)]) -> String {
    for (token, value) in values {
        text = text.replace(token, value);
    }
    text
}
macro_rules! hidden {
    ($text:expr) => {{
        const TEXT: &str = $text;
        const DATA: [u8; TEXT.len()] =
            $crate::presets::opaque::encode::<{ TEXT.len() }>(TEXT.as_bytes());
        $crate::presets::opaque::decode(&DATA)
    }};
}
macro_rules! render {
    ($text:expr, $($token:literal => $value:expr),+ $(,)?) => {
        $crate::presets::opaque::substitute($crate::presets::opaque::hidden!($text), &[$(($token, $value)),+])
    };
}
pub(crate) use {hidden, render};
