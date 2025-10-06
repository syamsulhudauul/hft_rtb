use anyhow::{anyhow, Result};
use bytes::{Buf, Bytes};
use bumpalo::Bump;
use common::{decode_tick, Tick};
use once_cell::sync::Lazy;
use parking_lot::Mutex;

static SCRATCH: Lazy<Mutex<Bump>> = Lazy::new(|| Mutex::new(Bump::new()));

pub fn decode_frame<'a>(frame: &'a [u8]) -> Result<Tick<'a>> {
    decode_tick(frame).map_err(|e| anyhow!("{e}"))
}

pub fn decode_bytes(bytes: Bytes) -> Result<Tick<'static>> {
    let len = bytes.len();
    let mut slice = bytes.clone();
    let owned = slice.copy_to_bytes(len);
    decode_tick(&owned)?.owned()
}

pub fn decode_batch<'a>(frames: impl Iterator<Item = &'a [u8]>) -> Result<Vec<Tick<'a>>> {
    frames.map(decode_frame).collect()
}

pub fn cache_aligned_buffer(size: usize) -> Vec<u8> {
    let mut bump = SCRATCH.lock();
    let slice = bump.alloc_slice_fill_with(size, |_| 0u8);
    slice.to_vec()
}
