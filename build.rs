use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
struct BlockData {
    name: String,
    hardness: f32,
    resistance: f32,
    drops: Vec<u16>
}

fn main() {
    let json = std::fs::read_to_string("src/runtime_data/block_data.json").unwrap();
    let data: Vec<BlockData> = serde_json::from_str(&json).unwrap();
    let map: HashMap<String, BlockData> =
        data.into_iter().map(|v| (v.name.clone(), v)).collect();

    let encoded = postcard::to_allocvec(&map).unwrap();
    std::fs::write("src/runtime_data/block_data.bin", encoded).unwrap();

    println!("cargo:rerun-if-changed=block_data.json");
}
