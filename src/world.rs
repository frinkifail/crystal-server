use std::collections::HashSet;
use std::sync::Arc;
use std::thread;

use flume::{Receiver, Sender};
use noise::{NoiseFn, SuperSimplex};
use tracing::{info, warn};
use valence::entity::pig::PigEntityBundle;
use valence::prelude::*;
use valence::spawn::IsFlat;

use crate::components::{combat::SpawnPlayerEvent, core::{WorldSeed, new_crystal_message}};

pub const SPAWN_POS: DVec3 = DVec3::new(0.5, 200.0, 0.5);
const HEIGHT: u32 = 192;
const SEA_LEVEL: f64 = 47.0;
const CHUNK_SIZE: u32 = 128;
const REGION_SIZE: i32 = 8;
const Y_OFFSET: i32 = 80;

struct ChunkWorkerState {
    sender: Sender<(ChunkPos, UnloadedChunk)>,
    receiver: Receiver<ChunkPos>,
    // density: SuperSimplex,
    hilly: SuperSimplex,
    stone: SuperSimplex,
    gravel: SuperSimplex,
    grass: SuperSimplex,
    seed: u64,
}

#[derive(Resource)]
pub struct WorldState {
    /// chunks that need to be generated, chunks without a priority have already
    /// been sent to the thread pool.
    pub pending_chunks: HashSet<ChunkPos>,
    pub sender: Sender<ChunkPos>, // sends chunk positions to workers
    pub receiver: Receiver<(ChunkPos, UnloadedChunk)>, // receives finished chunks from workers
}

/// smaller = sent first (bc theyre closer)
// type Priority = u64;

pub fn setup_world(
    mut commands: Commands,
    server: Res<Server>,
    dimensions: Res<DimensionTypeRegistry>,
    biomes: Res<BiomeRegistry>,
    seed: Res<WorldSeed>,
) {
    info!("worldgen time");
    let seed = seed.0;
    info!("seed: {seed}");

    let (finished_sender, finished_receiver) = flume::unbounded();
    let (pending_sender, pending_receiver) = flume::unbounded();

    let worker_shared_state = Arc::new(ChunkWorkerState {
        sender: finished_sender,
        receiver: pending_receiver,
        // density: SuperSimplex::new(seed),
        hilly: SuperSimplex::new(seed.wrapping_add(1) as u32),
        stone: SuperSimplex::new(seed.wrapping_add(2) as u32),
        gravel: SuperSimplex::new(seed.wrapping_add(3) as u32),
        grass: SuperSimplex::new(seed.wrapping_add(4) as u32),
        seed,
    });

    let core_count = thread::available_parallelism().map_or(1, |p| p.get()); // if i use all it lowkey explodes
    // let core_count = 7;
    info!("using {core_count} threads");
    for _ in 0..core_count {
        let state_clone = worker_shared_state.clone();
        thread::spawn(move || chunk_worker(state_clone));
    }

    commands.insert_resource(WorldState {
        pending_chunks: HashSet::new(),
        sender: pending_sender,
        receiver: finished_receiver,
    });

    let layer = LayerBundle::new(ident!("overworld"), &dimensions, &biomes, &server);
    let layer_id = commands.spawn(layer).id();

    info!("world layer spawned");

    commands.spawn(PigEntityBundle {
        layer: EntityLayerId(layer_id),
        position: Position(DVec3::new(22.5, 64.0, -5.5)),
        ..Default::default()
    });
}

pub fn init_clients_world(
    mut clients: Query<
        (
            Entity,
            &mut EntityLayerId,
            &mut VisibleChunkLayer,
            &mut VisibleEntityLayers,
            &mut Position,
            &mut GameMode,
            &mut IsFlat,
            &mut Client,
            &Username,
        ),
        Added<Client>,
    >,
    layers: Query<Entity, (With<ChunkLayer>, With<EntityLayer>)>,
    mut events: EventWriter<SpawnPlayerEvent>
) {
    if layers.is_empty() {
        return;
    }

    let layer = layers.single();

    for (
        entity,
        mut layer_id,
        mut visible_chunk_layer,
        mut visible_entity_layers,
        mut pos,
        mut game_mode,
        mut is_flat,
        mut client,
        username
    ) in &mut clients
    {
        layer_id.0 = layer;
        visible_chunk_layer.0 = layer;
        visible_entity_layers.0.insert(layer);
        pos.set(SPAWN_POS);
        *game_mode = GameMode::Creative;
        is_flat.0 = false;

        client.send_chat_message(new_crystal_message(
            "Welcome to Crystal!".color(Color::GOLD),
        ));
        client.send_chat_message(format!("{} joined the party :3", username.0).color(Color::GREEN));

        events.send(SpawnPlayerEvent(entity));
    }
}

// removes chunks from memory when no players are viewing them
// rust is already super memorylight so this probably wont do anything unless you keep your server on for a year straight and invite a thousand players
pub fn _remove_unviewed_chunks(mut layers: Query<&mut ChunkLayer>) {
    let Ok(mut layer) = layers.get_single_mut() else {
        return;
    };

    layer.retain_chunks(|_pos, chunk| chunk.viewer_count() > 0);
}

pub fn update_client_views(
    layers: Query<&mut ChunkLayer>,
    mut clients: Query<(&mut Client, View, OldView)>,
    mut state: ResMut<WorldState>,
) {
    let Ok(layer) = layers.get_single() else {
        return;
    }; // use immutable borrow if layer isn't modified
    // not sure why but its good from what ive heard

    for (client, view, old_view) in &mut clients {
        let view = view.get();
        let old_view = old_view.get();

        let queue_pos = |pos: ChunkPos| {
            if layer.chunk(pos).is_none() && !state.pending_chunks.contains(&pos) {
                // convert chunk position to region position
                // its for experimental batch generation for faster large view distance gens
                let region_x = pos.x.div_euclid(REGION_SIZE as i32);
                let region_z = pos.z.div_euclid(REGION_SIZE as i32);
                let region_pos = ChunkPos::new(region_x, region_z);

                for cz in 0..REGION_SIZE {
                    for cx in 0..REGION_SIZE {
                        let chunk_pos = ChunkPos::new(
                            region_pos.x * REGION_SIZE + cx,
                            region_pos.z * REGION_SIZE + cz,
                        );
                        state.pending_chunks.insert(chunk_pos);
                    }
                }
                state.sender.try_send(region_pos).unwrap();

                // if !state.pending_chunks.contains(&region_pos) {
                //     state.pending_chunks.insert(region_pos);
                //     state.sender.try_send(region_pos).unwrap();
                // }
            }
        };

        if client.is_added() {
            view.iter().for_each(queue_pos);
        } else {
            if old_view != view {
                view.diff(old_view).for_each(queue_pos);
            }
        }
    }
}

// worker management
pub fn recv_chunks(mut layers: Query<&mut ChunkLayer>, mut state: ResMut<WorldState>) {
    let Ok(mut layer) = layers.get_single_mut() else {
        return;
    };

    let received_chunks: Vec<_> = state
        .receiver
        .try_iter()
        .take(state.receiver.len() / 100 + 5)
        .collect();
    for (pos, chunk) in received_chunks {
        // let Some(_) = state.pending.remove(&pos) else { warn!("found unrequested chunk at {pos:?}"); continue; };
        // assert!(prio_opt.is_none());
        if !state.pending_chunks.remove(&pos) {
            warn!("found unrequested chunk at {pos:?}");
        }
        layer.insert_chunk(pos, chunk);
    }
}

// v5 chunkgen
// fn chunk_worker(state: Arc<ChunkWorkerState>) {
//     while let Ok(pos) = state.receiver.recv() {
//         // experimental batch generation
//         let mut gravel_cache = vec![vec![0.0; CHUNK_SIZE as usize]; CHUNK_SIZE as usize];
//         let mut stone_cache = vec![vec![0.0; CHUNK_SIZE as usize]; CHUNK_SIZE as usize];

//         for z in 0..CHUNK_SIZE {
//             for x in 0..CHUNK_SIZE {
//                 let wx = (pos.x * CHUNK_SIZE as i32) + x as i32;
//                 let wz = (pos.z * CHUNK_SIZE as i32) + z as i32;
//                 let p = DVec3::new(wx as f64, 0.0, wz as f64);

//                 gravel_cache[z as usize][x as usize] = fbm(&state.gravel, p / 10.0, 3, 2.0, 0.5);
//                 stone_cache[z as usize][x as usize] = noise01(&state.stone, p / 15.0);
//             }
//         }

//         // 8x8 region split
//         // each worker does more instead of spawning 999 workers that use like 2% CPU
//         for cz in 0..REGION_SIZE {
//             for cx in 0..REGION_SIZE {
//                 let mut chunk = UnloadedChunk::with_height(HEIGHT);

//                 // 16x16 chunk
//                 for z in 0..16 {
//                     for x in 0..16 {
//                         let cache_x = (cx * 16 + x) as usize;
//                         let cache_z = (cz * 16 + z) as usize;
//                         let gravel_noise = gravel_cache[cache_z][cache_x];
//                         let stone_noise = stone_cache[cache_z][cache_x];

//                         let wx = (pos.x * CHUNK_SIZE as i32) + cache_x as i32;
//                         let wz = (pos.z * CHUNK_SIZE as i32) + cache_z as i32;
//                         let p_col = DVec3::new(wx as f64, 0.0, wz as f64);

//                         let gravel_height =
//                             ((55.0 - 1.0 - (gravel_noise * 6.0)) as f64).floor() as i32;
//                         let hilly = lerp(-2.0, 2.0, noise01(&state.hilly, p_col / 80.0));
//                         let surface_height = (SEA_LEVEL + 5.0 + (hilly * 30.0)) as i32;

//                         // println!("{surface_height}");

//                         for y in (0..HEIGHT as i32).rev() {
//                             // println!("{y}");
//                             let mut block = BlockState::AIR;

//                             if y <= surface_height {
//                                 if y == surface_height {
//                                     block = if y < gravel_height {
//                                         BlockState::GRAVEL
//                                     } else {
//                                         BlockState::GRASS_BLOCK
//                                     };
//                                 } else if y
//                                     > (((surface_height as f64 - (stone_noise * 5.0) as f64)
//                                         .max(1.0)
//                                         .round()) as i32)
//                                 {
//                                     block = if y < gravel_height {
//                                         BlockState::GRAVEL
//                                     } else {
//                                         BlockState::DIRT
//                                     };
//                                 } else {
//                                     block = BlockState::STONE;
//                                 }
//                             } else if y < SEA_LEVEL as i32 {
//                                 block = BlockState::WATER;
//                             }

//                             chunk.set_block_state(
//                                 x as u32,
//                                 ((y + Y_OFFSET) as u32).min(HEIGHT - 1),
//                                 z as u32,
//                                 block,
//                             );
//                         }

//                         if surface_height > 1 {
//                             let sy = surface_height as u32;
//                             if chunk.block_state(
//                                 x as u32,
//                                 (sy + Y_OFFSET as u32).min(HEIGHT - 1),
//                                 z as u32,
//                             ) == BlockState::GRASS_BLOCK
//                             {
//                                 let py = sy + 1;
//                                 if py + 1 < HEIGHT {
//                                     let density = fbm(
//                                         &state.grass,
//                                         DVec3::new(wx as f64, surface_height as f64, wz as f64)
//                                             / 5.0,
//                                         4,
//                                         2.0,
//                                         0.7,
//                                     );
//                                     if density > 0.55 {
//                                         if density > 0.7 {
//                                             let upper = BlockState::TALL_GRASS
//                                                 .set(PropName::Half, PropValue::Upper);
//                                             let lower = BlockState::TALL_GRASS
//                                                 .set(PropName::Half, PropValue::Lower);
//                                             chunk.set_block_state(
//                                                 x as u32,
//                                                 (py + Y_OFFSET as u32 + 1).min(HEIGHT - 1),
//                                                 z as u32,
//                                                 upper,
//                                             );
//                                             chunk.set_block_state(
//                                                 x as u32,
//                                                 (py + Y_OFFSET as u32).min(HEIGHT - 1),
//                                                 z as u32,
//                                                 lower,
//                                             );
//                                         } else {
//                                             chunk.set_block_state(
//                                                 x as u32,
//                                                 (py + Y_OFFSET as u32).min(HEIGHT - 1),
//                                                 z as u32,
//                                                 BlockState::GRASS,
//                                             );
//                                         }
//                                     }
//                                 }
//                             }
//                             // let ground_y = (sy + Y_OFFSET as u32).min(HEIGHT - 1);

//                             for dx in -2..=2 {
//                                 for dz in -2..=2 {
//                                     let rx = (cx * 16 + x as i32) + dx;
//                                     let rz = (cz * 16 + z as i32) + dz;

//                                     if rx < 0 || rx >= 128 || rz < 0 || rz >= 128 {
//                                         continue;
//                                     }

//                                     let world_x = (pos.x * 128) + rx;
//                                     let world_z = (pos.z * 128) + rz;

//                                     // Seed the RNG deterministically based on the tree's potential center
//                                     // let seed = ((world_x as u64) << 32) | (world_z as u64 & 0xFFFFFFFF);
//                                     let mut rng = StdRng::seed_from_u64(state.seed);

//                                     if rng.gen_bool(0.001) {
//                                         // 0.1% chance
//                                         let tree_height = rng.gen_range(4..7);

//                                         if dx == 0 && dz == 0 {
//                                             for ty in 1..=tree_height {
//                                                 let py =
//                                                     (sy + ty + Y_OFFSET as u32).min(HEIGHT - 1);
//                                                 chunk.set_block_state(
//                                                     x as u32,
//                                                     py,
//                                                     z as u32,
//                                                     BlockState::OAK_LOG,
//                                                 );
//                                             }
//                                         }

//                                         for ly in (tree_height - 2)..=(tree_height + 1) {
//                                             let radius = if ly >= tree_height { 1 } else { 2 };
//                                             if dx.abs() <= radius && dz.abs() <= radius {
//                                                 let py =
//                                                     (sy + ly + Y_OFFSET as u32).min(HEIGHT - 1);
//                                                 if chunk.block_state(x as u32, py, z as u32)
//                                                     == BlockState::AIR
//                                                 {
//                                                     chunk.set_block_state(
//                                                         x as u32,
//                                                         py,
//                                                         z as u32,
//                                                         BlockState::OAK_LEAVES
//                                                             .set(PropName::Distance, PropValue::_1),
//                                                     );
//                                                 }
//                                             }
//                                         }
//                                     }
//                                 }
//                             }
//                         }
//                     }
//                 }

//                 let chunk_pos = ChunkPos::new(
//                     pos.x * REGION_SIZE + cx as i32,
//                     pos.z * REGION_SIZE + cz as i32,
//                 );

//                 // Generate trees occasionally
//                 // if fastrand::u32(0..100) < 5 { // 5% chance per column
//                 //     // Find the highest solid block at this chunk position
//                 //     let mut surface_y = 0;
//                 //     for y in (0..HEIGHT as i32).rev() {
//                 //         let block = chunk.block_state(0, y as u32, 0);
//                 //         if block != BlockState::AIR && block != BlockState::WATER {
//                 //             surface_y = y;
//                 //             break;
//                 //         }
//                 //     }

//                 //     // Generate tree on top of surface if it's reasonable height
//                 //     if surface_y > 0 && surface_y < HEIGHT as i32 - 10 {
//                 //         let tree_x = 8; // Center of 16x16 chunk
//                 //         let tree_z = 8; // Center of 16x16 chunk
//                 //         let tree_y = surface_y as u32;
//                 //         generate_tree(&mut chunk, tree_x, tree_y, tree_z);
//                 //     }
//                 // }

//                 if let Err(e) = state.sender.try_send((chunk_pos, chunk)) {
//                     warn!("failed to send chunk {:?}: {}", chunk_pos, e);
//                 }
//             }
//         }
//         thread::yield_now();
//     }
// }
// v6 chunkgen
fn chunk_worker(state: Arc<ChunkWorkerState>) {
    while let Ok(pos) = state.receiver.recv() {
        let mut gravel_cache = vec![0.0f64; CHUNK_SIZE as usize * CHUNK_SIZE as usize].into_boxed_slice();
        let mut stone_cache = vec![0.0f64; CHUNK_SIZE as usize * CHUNK_SIZE as usize].into_boxed_slice();
        // let mut gravel_cache = vec![vec![0.0; CHUNK_SIZE as usize]; CHUNK_SIZE as usize];
        // let mut stone_cache = vec![vec![0.0; CHUNK_SIZE as usize]; CHUNK_SIZE as usize];

        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let wx = (pos.x * CHUNK_SIZE as i32) + x as i32;
                let wz = (pos.z * CHUNK_SIZE as i32) + z as i32;
                let p = DVec3::new(wx as f64, 0.0, wz as f64);

                gravel_cache[(z * CHUNK_SIZE + x) as usize] = fbm(&state.gravel, p / 10.0, 3, 2.0, 0.5);
                stone_cache[(z * CHUNK_SIZE + x) as usize] = noise01(&state.stone, p / 15.0);
                // gravel_cache[z as usize][x as usize] = fbm(&state.gravel, p / 10.0, 3, 2.0, 0.5);
                // stone_cache[z as usize][x as usize] = noise01(&state.stone, p / 15.0);
            }
        }

        // 2. Iterate through the 8x8 chunks in this region
        for cz in 0..REGION_SIZE {
            for cx in 0..REGION_SIZE {
                let mut chunk = UnloadedChunk::with_height(HEIGHT);

                for z in 0..16 {
                    for x in 0..16 {
                        let cache_x = (cx * 16 + x as i32) as usize;
                        let cache_z = (cz * 16 + z as i32) as usize;

                        let wx = (pos.x * CHUNK_SIZE as i32) + cache_x as i32;
                        let wz = (pos.z * CHUNK_SIZE as i32) + cache_z as i32;
                        let p_col = DVec3::new(wx as f64, 0.0, wz as f64);

                        let gravel_noise = gravel_cache[cache_z * CHUNK_SIZE as usize + cache_x];
                        let stone_noise = stone_cache[cache_z * CHUNK_SIZE as usize + cache_x];

                        let gravel_height = ((55.0 - 1.0 - (gravel_noise * 6.0)) as f64).floor() as i32;
                        let hilly = lerp(-2.0, 2.0, noise01(&state.hilly, p_col / 80.0));
                        let surface_height = (SEA_LEVEL + 5.0 + (hilly * 30.0)) as i32;

                        // Terrain Layering
                        for y in (0..HEIGHT as i32).rev() {
                            let mut block = BlockState::AIR;
                            let adj_y = y + Y_OFFSET;
                            if adj_y < 0 || adj_y >= HEIGHT as i32 { continue; }

                            if y <= surface_height {
                                if y == surface_height {
                                    block = if y < gravel_height { BlockState::GRAVEL } else { BlockState::GRASS_BLOCK };
                                } else if y > (surface_height as f64 - (stone_noise * 5.0).max(1.0)).round() as i32 {
                                    block = if y < gravel_height { BlockState::GRAVEL } else { BlockState::DIRT };
                                } else {
                                    block = BlockState::STONE;
                                }
                            } else if y < SEA_LEVEL as i32 {
                                block = BlockState::WATER;
                            }

                            chunk.set_block_state(x as u32, adj_y as u32, z as u32, block);
                        }

                        // Grass and Tree Logic
                        if surface_height > 1 {
                            let py_base = (surface_height + Y_OFFSET + 1) as u32;
                            if py_base >= HEIGHT { continue; }

                            // Check if current block is grass to place foliage
                            if chunk.block_state(x as u32, py_base - 1, z as u32) == BlockState::GRASS_BLOCK {
                                let density = fbm(&state.grass, DVec3::new(wx as f64, surface_height as f64, wz as f64) / 5.0, 4, 2.0, 0.7);
                                if density > 0.55 && py_base + 1 < HEIGHT {
                                    if density > 0.7 {
                                        chunk.set_block_state(x as u32, py_base + 1, z as u32, BlockState::TALL_GRASS.set(PropName::Half, PropValue::Upper));
                                        chunk.set_block_state(x as u32, py_base, z as u32, BlockState::TALL_GRASS.set(PropName::Half, PropValue::Lower));
                                    } else {
                                        chunk.set_block_state(x as u32, py_base, z as u32, BlockState::GRASS);
                                    }
                                }
                            }

                            // Sparse Tree Generation using a 5x5 neighborhood check
                            for dx in -2..=2 {
                                for dz in -2..=2 {
                                    let nx = (cx * 16 + x as i32) + dx;
                                    let nz = (cz * 16 + z as i32) + dz;

                                    // Keep it within the 128x128 region boundaries
                                    if nx < 0 || nx >= 128 || nz < 0 || nz >= 128 { continue; }

                                    let n_world_x = (pos.x * 128) + nx;
                                    let n_world_z = (pos.z * 128) + nz;

                                    // Simple, fast deterministic hash for tree centers
                                    let h = (n_world_x.wrapping_mul(3121) ^ n_world_z.wrapping_mul(4391)).wrapping_add(state.seed as i32).abs() as u32;

                                    if (h % 1000) < 10 { // Roughly 0.06% chance
                                        let tree_height = 4 + (h % 3);

                                        // Trunk
                                        if dx == 0 && dz == 0 {
                                            for ty in 0..tree_height {
                                                let ly = (py_base + ty).min(HEIGHT - 1);
                                                if chunk.block_state(x as u32, ly, z as u32) == BlockState::AIR {
                                                    chunk.set_block_state(x as u32, ly, z as u32, BlockState::OAK_LOG);
                                                }
                                            }
                                        }

                                        // Leaves
                                        for ly in (tree_height - 2)..=(tree_height + 1) {
                                            let radius = if ly >= tree_height { 1 } else { 2 } as i32;
                                            if dx.abs() <= radius && dz.abs() <= radius {
                                                let leaf_y = (py_base + ly).min(HEIGHT - 1);
                                                if chunk.block_state(x as u32, leaf_y, z as u32) == BlockState::AIR {
                                                    chunk.set_block_state(x as u32, leaf_y, z as u32, BlockState::OAK_LEAVES.set(PropName::Distance, PropValue::_1));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                let chunk_pos = ChunkPos::new(
                    pos.x * REGION_SIZE + cx as i32,
                    pos.z * REGION_SIZE + cz as i32,
                );

                if let Err(e) = state.sender.try_send((chunk_pos, chunk)) {
                    warn!("failed to send chunk {:?}: {}", chunk_pos, e);
                }
            }
        }
        thread::yield_now();
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a * (1.0 - t) + b * t
}

fn fbm(noise: &SuperSimplex, p: DVec3, octaves: u32, lacunarity: f64, persistence: f64) -> f64 {
    let mut freq = 1.0;
    let mut amp = 1.0;
    let mut amp_sum = 0.0;
    let mut sum = 0.0;
    for _ in 0..octaves {
        let n = noise01(noise, p * freq);
        sum += n * amp;
        amp_sum += amp;
        freq *= lacunarity;
        amp *= persistence;
    }
    sum / amp_sum
}

fn noise01(noise: &SuperSimplex, p: DVec3) -> f64 {
    (noise.get(p.to_array()) + 1.0) / 2.0
}

// fn generate_tree(chunk: &mut UnloadedChunk, x: u32, y: u32, z: u32) {
//     // Simple oak tree generation
//     let trunk_height = 4 + (fastrand::u8(0..3) as u32); // 4-6 blocks tall

//     // Generate trunk
//     for ty in 0..trunk_height {
//         let world_y = y + ty;
//         if world_y < HEIGHT {
//             chunk.set_block_state(x, world_y, z, BlockState::OAK_LOG);
//         }
//     }

//     // Generate leaves (sphere-like shape)
//     let leaf_y_start = y + trunk_height;
//     let leaf_radius = 3;

//     for lx in -leaf_radius..=leaf_radius {
//         for ly in -leaf_radius..=leaf_radius {
//             for lz in -leaf_radius..=leaf_radius {
//                 let dx = lx as i32;
//                 let dy = ly as i32;
//                 let dz = lz as i32;

//                 // Sphere equation: dx^2 + dy^2 + dz^2 <= radius^2
//                 if dx * dx + dy * dy + dz * dz <= leaf_radius * leaf_radius {
//                     let world_x = x.wrapping_add_signed(lx);
//                     let world_y = leaf_y_start.wrapping_add_signed(ly);
//                     let world_z = z.wrapping_add_signed(lz);

//                     // Check bounds
//                     if world_x < 16 && world_y < HEIGHT && world_z < 16 {
//                         // Only place leaves if not replacing wood
//                         let current_block = chunk.block_state(world_x, world_y, world_z);
//                         if current_block == BlockState::AIR || current_block == BlockState::GRASS {
//                             chunk.set_block_state(
//                                 world_x,
//                                 world_y,
//                                 world_z,
//                                 BlockState::OAK_LEAVES,
//                             );
//                         }
//                     }
//                 }
//             }
//         }
//     }
// }
