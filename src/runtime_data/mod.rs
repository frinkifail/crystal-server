#![allow(dead_code)]

use std::{collections::HashMap, sync::LazyLock};
use tracing::info;
use serde::Deserialize;
use valence::{ItemKind, block::BlockKind, rand::seq::SliceRandom};

#[derive(Deserialize)]
struct BlockData {
    name: String,
    hardness: f32,
    resistance: f32,
    drops: Vec<u16>
}

const BLOCK_DATA: &[u8] = include_bytes!("block_data.bin");

static DATA_MAP: LazyLock<HashMap<String, BlockData>> = LazyLock::new(|| {
    let start = std::time::Instant::now();
    let data = postcard::from_bytes(BLOCK_DATA).unwrap();
    info!("loading data took {:?}ms", start.elapsed().as_millis_f32());
    data
});

static HARDNESS_MAP: LazyLock<HashMap<String, f32>> = LazyLock::new(|| {
    DATA_MAP.iter().map(|(k, v)| (k.to_owned(), v.hardness)).collect()
});
static RESISTANCE_MAP: LazyLock<HashMap<String, f32>> = LazyLock::new(|| {
    DATA_MAP.iter().map(|(k, v)| (k.to_owned(), v.resistance)).collect()
});
static DROPS_MAP: LazyLock<HashMap<String, Vec<u16>>> = LazyLock::new(|| {
    DATA_MAP.iter().map(|(k, v)| (k.to_owned(), v.drops.clone())).collect()
});

pub trait HasExtraBlockData {
    fn hardness(&self) -> f32;
    fn resistance(&self) -> f32;
    fn drops(&self) -> &Vec<u16>;
    fn random_drop(&self) -> Option<ItemKind>;
}

impl HasExtraBlockData for BlockKind {
    /// Fetches the hardness of the block.
    ///
    /// A value of `-1.0` means no corresponding value was found.
    fn hardness(&self) -> f32 {
        HARDNESS_MAP.get(self.to_str()).copied().unwrap_or(-1.0)
    }
    /// Fetches the blast resistance of the block.
    ///
    /// A value of `-1.0` means no corresponding value was found.
    fn resistance(&self) -> f32 {
        RESISTANCE_MAP.get(self.to_str()).copied().unwrap_or(-1.0)
    }
    /// Fetches what items this block can drop.
    fn drops(&self) -> &Vec<u16> {
        DROPS_MAP.get(self.to_str()).unwrap()
    }
    /// Gets a random drop from this block.
    fn random_drop(&self) -> Option<ItemKind> {
        self.drops().choose(&mut valence::rand::thread_rng()).and_then(|v| ItemKind::from_raw(*v))
    }
}
