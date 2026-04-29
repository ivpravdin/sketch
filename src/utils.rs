use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn fnv1a_hash(input: &[u8]) -> u32 {
    const FNV_PRIME: u32 = 16777619;
    const FNV_OFFSET_BASIS: u32 = 2166136261;

    let mut hash = FNV_OFFSET_BASIS;

    for &byte in input {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    hash
}

// Time + PID
pub fn session_id() -> String {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    let pid = process::id() as u64;

    let mut x = t ^ pid;
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;

    format!("{:04x}", x & 0xffff)
}
