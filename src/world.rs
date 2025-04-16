// src/world.rs

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;
use std::thread;
use std::time::SystemTime;

use flume::{Receiver, Sender};
use noise::{NoiseFn, SuperSimplex};
use tracing::info;
use valence::command::scopes::CommandScopes;
use valence::message::SendMessage;
use valence::op_level::OpLevel;
// Needed for init_clients_world messages
use valence::prelude::*;
use valence::spawn::IsFlat;

use crate::components::core::set_op_status; // Import for OP status

// --- Constants ---
pub const SPAWN_POS: DVec3 = DVec3::new(0.5, 200.0, 0.5); // Centered in block, high up
const HEIGHT: u32 = 384; // World height

// --- Structs and Types ---

// State shared between chunk generation worker threads
struct ChunkWorkerState {
    sender: Sender<(ChunkPos, UnloadedChunk)>,
    receiver: Receiver<ChunkPos>,
    // Noise functions
    density: SuperSimplex,
    hilly: SuperSimplex,
    stone: SuperSimplex,
    gravel: SuperSimplex,
    grass: SuperSimplex,
}

// Resource holding the state for queuing and receiving generated chunks
#[derive(Resource)]
pub struct GameState {
    /// Chunks that need to be generated. Chunks without a priority have already
    /// been sent to the thread pool.
    pending: HashMap<ChunkPos, Option<Priority>>,
    sender: Sender<ChunkPos>, // Sends chunk positions TO workers
    receiver: Receiver<(ChunkPos, UnloadedChunk)>, // Receives finished chunks FROM workers
}

/// The order in which chunks should be processed by the thread pool. Smaller
/// values are sent first (closer chunks).
type Priority = u64;

// --- Setup Function ---

pub fn setup_world(
    mut commands: Commands,
    server: Res<Server>,
    dimensions: Res<DimensionTypeRegistry>,
    biomes: Res<BiomeRegistry>,
) {
    info!("Setting up procedural world generation...");
    let seconds_per_day = 86_400;
    let seed = (SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        / seconds_per_day) as u32;

    info!("Using generation seed: {seed}");

    let (finished_sender, finished_receiver) = flume::unbounded();
    let (pending_sender, pending_receiver) = flume::unbounded();

    let worker_shared_state = Arc::new(ChunkWorkerState {
        sender: finished_sender,
        receiver: pending_receiver,
        density: SuperSimplex::new(seed),
        hilly: SuperSimplex::new(seed.wrapping_add(1)),
        stone: SuperSimplex::new(seed.wrapping_add(2)),
        gravel: SuperSimplex::new(seed.wrapping_add(3)),
        grass: SuperSimplex::new(seed.wrapping_add(4)),
    });

    // Start worker threads
    // let core_count = thread::available_parallelism().map_or(1, |p| p.get());
    let core_count = 8;
    info!("Spawning {} chunk generation worker threads...", core_count);
    for _ in 0..core_count {
        let state_clone = worker_shared_state.clone();
        thread::spawn(move || chunk_worker(state_clone));
    }

    // Insert GameState resource for main thread communication
    commands.insert_resource(GameState {
        pending: HashMap::new(),
        sender: pending_sender,
        receiver: finished_receiver,
    });

    // Spawn the main world layer entity
    let layer = LayerBundle::new(ident!("overworld"), &dimensions, &biomes, &server);
    commands.spawn(layer);

    info!("World layer spawned.");
}

// --- World-Related Systems ---

// Initializes clients specifically for this world type
pub fn init_clients_world(
    mut clients: Query<
        (
            &mut EntityLayerId,
            &mut VisibleChunkLayer,
            &mut VisibleEntityLayers,
            &mut Position,
            &mut GameMode,
            &mut IsFlat,
            &mut Client,
            &Username,
            &mut OpLevel,
            &mut CommandScopes,
        ),
        Added<Client>,
    >,
    layers: Query<Entity, (With<ChunkLayer>, With<EntityLayer>)>,
) {
    if layers.is_empty() {
        return;
    }

    let layer = layers.single();

    for (
        mut layer_id,
        mut visible_chunk_layer,
        mut visible_entity_layers,
        mut pos,
        mut game_mode,
        mut is_flat,
        mut client,
        username,
        mut op_level,
        mut permissions,
    ) in &mut clients
    {
        layer_id.0 = layer;
        visible_chunk_layer.0 = layer;
        visible_entity_layers.0.insert(layer);
        pos.set(SPAWN_POS);
        *game_mode = GameMode::Creative;
        is_flat.0 = false;

        client.send_chat_message(
            "[Crystal] ".color(Color::RED) + "Welcome to Crystal!".color(Color::GOLD),
        );
        client.send_chat_message(format!("{} joined the party :3", username.0).color(Color::GREEN));
        set_op_status(
            &mut client,
            username,
            &mut op_level,
            Some(true),
            &mut permissions,
        );

        info!(
            "[world] {} initialized in world at {:?}",
            username.0, SPAWN_POS
        );
    }
}

// Removes chunks from memory when no players are viewing them
pub fn remove_unviewed_chunks(mut layers: Query<&mut ChunkLayer>) {
    let Ok(mut layer) = layers.get_single_mut() else {
        return;
    };

    layer.retain_chunks(|_pos, chunk| chunk.viewer_count() > 0);
}

// Queues chunks to be generated based on player view distance changes
pub fn update_client_views(
    layers: Query<&mut ChunkLayer>, // Change to immutable borrow if possible
    mut clients: Query<(&mut Client, View, OldView)>, // Removed mut Client here
    mut state: ResMut<GameState>,
) {
    let Ok(layer) = layers.get_single() else {
        return;
    }; // Use immutable borrow if layer isn't modified

    for (client, view, old_view) in &mut clients {
        // Use _client if not needed directly
        let view = view.get();
        let old_view = old_view.get(); // Get old view unconditionally

        // Function to queue a chunk position if needed
        let queue_pos = |pos: ChunkPos| {
            if layer.chunk(pos).is_none() {
                // Check if chunk doesn't exist yet
                match state.pending.entry(pos) {
                    // Already pending? Update priority if current view is closer.
                    Entry::Occupied(mut oe) => {
                        if let Some(priority) = oe.get_mut() {
                            let dist = view.pos.distance_squared(pos);
                            *priority = (*priority).min(dist);
                        }
                        // If priority is None, it's already sent to worker, do nothing.
                    }
                    // Not pending? Add it with current view distance priority.
                    Entry::Vacant(ve) => {
                        let dist = view.pos.distance_squared(pos);
                        ve.insert(Some(dist));
                    }
                }
            }
        };

        // Queue all the new chunks in the view to be sent to the thread pool.
        if client.is_added() {
            view.iter().for_each(queue_pos);
        } else {
            if old_view != view {
                view.diff(old_view).for_each(queue_pos);
            }
        }
    }
}

// Sends pending chunks to workers and receives/inserts finished chunks
pub fn send_recv_chunks(mut layers: Query<&mut ChunkLayer>, mut state: ResMut<GameState>) {
    let Ok(mut layer) = layers.get_single_mut() else {
        return;
    };

    // Insert the chunks that are finished generating into the instance.
    let received_chunks: Vec<_> = state.receiver.try_iter().collect(); // Collect into a temporary variable
    for (pos, chunk) in received_chunks {
        if let Some(prio_opt) = state.pending.remove(&pos) {
            if prio_opt.is_none() { // Ensure it was actually sent (priority was None)
                // Inside the `if prio_opt.is_none()` block:
                info!("Attempting to insert chunk at {:?}", pos); // Log *before* calling
                layer.insert_chunk(pos, chunk);
                info!("Successfully called insert_chunk for {:?}", pos); // Log *after* calling
            } else {
                // Chunk finished but shouldn't have? Log warning.
                info!("Received chunk {:?} that still had priority?", pos);
                println!("THIS IS FUCKING BEING HIT");
                state.pending.insert(pos, prio_opt); // Put it back? Or just discard?
                panic!("LITERALLY MAX CONFIRMATION");
            }
        } else {
            // Received a chunk that wasn't pending? Should not happen.
            info!("Received unexpected chunk {:?}", pos);
            panic!("I SWEAR THIS ISNT HIT");
        }
    }
    // for (pos, chunk) in state.receiver.drain() {
    //     layer.insert_chunk(pos, chunk);
    //     // assert!(state.pending.remove(&pos).is_some());
    // }

    // Collect chunks that have a priority set (ready to be sent).
    let mut to_send: Vec<(Priority, ChunkPos)> = Vec::new();
    for (pos, priority) in &mut state.pending {
        if let Some(pri) = priority.take() {
            // Take the priority, leaving None (marks as sent)
            to_send.push((pri, *pos));
        }
    }

    // Sort chunks by ascending priority (distance).
    to_send.sort_unstable_by_key(|(pri, _)| *pri);

    // Send the sorted chunks to the worker pool.
    for (_, pos) in to_send {
        if let Err(e) = state.sender.try_send(pos) {
            // Failed to send (channel closed or full?). Log and put priority back.
            info!("Failed to send chunk {:?} to worker: {}", pos, e);
            if let Some(prio_opt) = state.pending.get_mut(&pos) {
                *prio_opt = Some(0); // Put back with some priority? Or remove?
            }
        }
    }
}

// --- Chunk Generation Worker ---

fn chunk_worker(state: Arc<ChunkWorkerState>) {
    while let Ok(pos) = state.receiver.recv() {
        // Blocking receive
        let mut chunk = UnloadedChunk::with_height(HEIGHT);

        // let mut blocks_set = 0;
        // let mut solid_blocks_check = 0;

        for z in 0..16 {
            for x in 0..16 {
                let world_x = (pos.x * 16) + x as i32;
                let world_z = (pos.z * 16) + z as i32;

                let mut in_terrain = false;
                let mut surface_depth = 0; // Tracks depth from the first solid block downwards

                // Generate column from top to bottom
                for y in (0..HEIGHT as i32).rev() {
                    let p = DVec3::new(world_x as f64, y as f64, world_z as f64);
                    const WATER_HEIGHT: i32 = 55;

                    let is_terrain = has_terrain_at(&state, p);
                    let block;

                    if is_terrain {
                        // blocks_set += 1;
                        let gravel_height = WATER_HEIGHT
                            - 1
                            - (fbm(&state.gravel, p / 10.0, 3, 2.0, 0.5) * 6.0).floor() as i32;

                        if !in_terrain {
                            // First solid block encountered from top
                            in_terrain = true;
                            // Determine surface depth based on noise
                            let stone_noise = noise01(&state.stone, p / 15.0);
                            surface_depth = (stone_noise * 5.0).max(1.0).round() as u32; // Ensure at least 1 block deep

                            if y < gravel_height {
                                block = BlockState::GRAVEL;
                            } else if y < WATER_HEIGHT {
                                // Allow dirt/grass below water level if near surface
                                block = BlockState::DIRT; // Changed from GRAVEL
                            } else {
                                // Threshold for grass block
                                block = BlockState::GRASS_BLOCK;
                            }
                        } else {
                            // Below the first solid block
                            if surface_depth > 0 {
                                surface_depth -= 1;
                                if y < gravel_height {
                                    // Prioritize gravel at lower depths
                                    block = BlockState::GRAVEL;
                                } else {
                                    block = BlockState::DIRT; // Below surface = dirt
                                }
                            } else {
                                block = BlockState::STONE; // Deep underground = stone
                            }
                        }
                    } else {
                        // No terrain at this Y level
                        in_terrain = false;
                        surface_depth = 0;
                        if y < WATER_HEIGHT {
                            block = BlockState::WATER;
                        } else {
                            block = BlockState::AIR;
                        }
                    }

                    chunk.set_block_state(x, y as u32, z, block);

                    // if !chunk.block_state(x, y as u32, z).is_air() {
                    //     solid_blocks_check += 1;
                    // }
                } // End Y loop

                // Add grass/tall grass decoration after terrain pass
                for y in 1..HEIGHT {
                    // Start from Y=1
                    let current_block = chunk.block_state(x, y, z);
                    let block_below = chunk.block_state(x, y - 1, z);

                    if current_block.is_air() && block_below == BlockState::GRASS_BLOCK {
                        let p = DVec3::new(world_x as f64, y as f64, world_z as f64);
                        let density = fbm(&state.grass, p / 5.0, 4, 2.0, 0.7);

                        if density > 0.55 {
                            if density > 0.7
                                && y + 1 < HEIGHT
                                && chunk.block_state(x, y + 1, z).is_air()
                            {
                                let upper =
                                    BlockState::TALL_GRASS.set(PropName::Half, PropValue::Upper);
                                let lower =
                                    BlockState::TALL_GRASS.set(PropName::Half, PropValue::Lower);
                                chunk.set_block_state(x, y + 1, z, upper);
                                chunk.set_block_state(x, y, z, lower);
                            } else {
                                chunk.set_block_state(x, y, z, BlockState::GRASS);
                            }
                        }
                    }
                } // End decoration Y loop
            } // End X loop
        } // End Z loop

        // info!("blocks: {}", blocks_set);

        // info!(
        //     "Worker finishing chunk {:?}. Calculated blocks: {}. Final solid check: {}",
        //     pos, blocks_set, solid_blocks_check
        // );

        // Send the finished chunk back to the main thread
        if let Err(e) = state.sender.try_send((pos, chunk)) {
            info!(
                "Failed to send finished chunk {:?} back to main thread: {}",
                pos, e
            );
        }
    }
    info!("Chunk worker thread shutting down.");
}

// --- Noise Helper Functions ---

fn has_terrain_at(state: &ChunkWorkerState, p: DVec3) -> bool {
    let hilly = lerp(0.1, 1.0, noise01(&state.hilly, p / 400.0)).powi(2);

    let lower = 15.0 + 100.0 * hilly;
    let upper = lower + 100.0 * hilly;

    if p.y <= lower {
        true
    } else if p.y >= upper {
        false
    } else {
        let density = 1.0 - lerpstep(lower, upper, p.y);
        let n = fbm(&state.density, p / 100.0, 4, 2.0, 0.5);
        // info!("N: {} Density: {}", n, density);
        // p.y < 64.0
        n < density
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a * (1.0 - t) + b * t
}

fn lerpstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0)
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

    sum / amp_sum // Already scaled to [0, 1]
}

fn noise01(noise: &SuperSimplex, p: DVec3) -> f64 {
    // SuperSimplex output is roughly [-1, 1]
    (noise.get(p.to_array()) + 1.0) / 2.0
}
