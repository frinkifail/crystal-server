#![allow(clippy::type_complexity)]
#![feature(duration_millis_float)]
#![feature(decl_macro)]

// #[cfg(not(debug_assertions))]
// use std::panic::PanicHookInfo;
// #[cfg(not(debug_assertions))]
// use valence::rand::prelude::SliceRandom;
use std::{collections::HashSet, fs, io, net::{Ipv4Addr, SocketAddrV4}, num::NonZeroU32, thread, time::{Instant, SystemTime}};

mod commands;
mod components;
mod world;
mod runtime_data;

use bevy_time::TimePlugin;
use commands::{
    core::{VersionCommand, handle_version_command},
    gamemode::{GamemodeCommand, handle_gamemode_command},
    op::{OpCommand, handle_op_command},
    teleport::{TeleportCommand, handle_teleport_command},
};
use components::{
    building::{digging, place_blocks},
    chat::chat_message_event,
    console::{ConsoleCommandEvent, ConsoleCommandReceiver, handle_console_command},
    core::ServerVersion,
    itempickup::item_pickup_system,
};
use crossbeam_channel::{Sender, unbounded};
use serde::Deserialize;
use tracing::{error, info};
use valence::{
    CompressionThreshold, ServerSettings, brand::SetBrand, command::{AddCommand, CommandScopeRegistry}, prelude::*
};

use crate::{commands::{
    core::{PingCommand, handle_ping_command}, give::{GiveCommand, handle_give_command}, setblock::{SetBlockCommand, handle_setblock_command}
}, components::{combat::{SpawnPlayerEvent, attacking, iframe_decay, remove_dead_entities, respawning, spawn_players}, core::{WorldSeed, delayed_despawn}, debug::{DebugCommand, DebugState, DebugStateComponent, handle_debug_command, tick_debug}, items::{ItemAddEvent, item_add_handler}}};

const VERSION: &str = "Alpha(dev)::0.8 (codebase changes)";
const SIMPLE_VERSION: &str = "Adev8";

// minecraft crash report ahh
// #[cfg(not(debug_assertions))]
// fn crash_handler(info: &PanicHookInfo) {
//     error!("[panic] panicked!");
//     let premessage = [
//         "&crystal::CrashLog",
//         ">> crystal crash log",
//         "Query<&CrashLog>",
//         "*crashlog",
//     ];
//     let comments = [
//         "not my fault",
//         "cat ate my homework",
//         "This is quite perplexing indeed.",
//         "working my ass",
//         "skill issue",
//     ];
//     let mut rng = valence::rand::thread_rng();
//     let location = info.location().unwrap();
//     let panic_text = format!(
//         "{}\n// {}\n{:?}: file '{}' line {}",
//         premessage.choose_mut(&mut rng).unwrap_or(&"crashed 3:"),
//         comments.choose_mut(&mut rng).unwrap_or(&"No comment."),
//         info.payload(),
//         location.file(),
//         location.line()
//     );
//     error!("{}", panic_text);
// }

#[derive(Deserialize)]
struct ServerConfig {
    online_mode: bool,
    ip: String,
    port: u16,
    incoming_byte_limit: usize,
    outgoing_byte_limit: usize,
    max_players: usize,
    tps: u32,
    compression_threshold: i32
}

fn main() {
    // settings
    let settings = fs::read_to_string("settings.toml").unwrap_or(r#"
online_mode = true
# x.x.x.x or blank
ip = ""
port = 25565
incoming_byte_limit = 2097152
outgoing_byte_limit = 8388608
max_players = 20
tps = 20
compression_threshold = 256
    "#.to_string());
    let Ok(config) = toml::from_str::<ServerConfig>(&settings) else { error!("failed to load config (missing fields?)"); return; };

    // better panic in release
    // i dont even know if it works because i never test release :p
    // #[cfg(not(debug_assertions))]
    // std::panic::set_hook(Box::new(crash_handler));

    let (tx, rx) = unbounded();
    start_console_input_thread(tx);

    let start = Instant::now();

    App::new()
        .insert_resource(ServerSettings {
            tick_rate: if let Some(tps) = NonZeroU32::new(config.tps) { tps } else {
                error!("invalid TPS setting, using 20.");
                unsafe { NonZeroU32::new_unchecked(20) }
            },
            compression_threshold: CompressionThreshold(config.compression_threshold)
        })
        .insert_resource(NetworkSettings {
            callbacks: ErasedNetworkCallbacks::default(),
            tokio_handle: None,
            max_connections: 1024.max(config.max_players + 128),
            max_players: config.max_players,
            address: SocketAddrV4::new(if config.ip.trim().is_empty() {
                Ipv4Addr::new(0, 0, 0, 0)
            } else {
                let Ok(res) = config.ip.parse::<Ipv4Addr>() else {
                    error!("failed to parse ip");
                    return;
                };
                res
            }, config.port).into(),
            connection_mode: if config.online_mode {
                ConnectionMode::Online { prevent_proxy_connections: false }
            } else {
                ConnectionMode::Offline
            },
            incoming_byte_limit: config.incoming_byte_limit, // 2 MiB
            outgoing_byte_limit: config.outgoing_byte_limit, // 8 MiB
        })
        .add_plugins(DefaultPlugins)
        .add_plugins((TimePlugin::default(),))
        .add_systems(
            Startup,
            (core_server_setup, world::setup_world, setup_core_commands),
        )
        .add_systems(PostStartup, move || {
            info!("finished init in {}ms", start.elapsed().as_millis_f32());
        })
        .add_systems(
            Update,
            (
                init_client,
                // world
                (
                    world::init_clients_world,
                    world::update_client_views,
                    world::recv_chunks,
                )
                    .chain(),
                // debug
                init_debug_component,
                tick_debug,
                // picking up
                item_pickup_system,
                // disconnecting
                despawn_disconnected_clients,
                leave_handler,
                // chatting
                chat_message_event,
                // the mine part of minecraft
                digging,
                place_blocks,
                // console
                poll_console_commands,
                handle_console_command,
                // commands
                (
                    handle_version_command,
                    handle_teleport_command,
                    handle_gamemode_command,
                    handle_op_command,
                    handle_ping_command,
                    handle_setblock_command,
                    handle_debug_command,
                    handle_give_command
                ).chain(),
                delayed_despawn,
                item_add_handler,
                // combat
                attacking,
                remove_dead_entities,
                respawning,
                spawn_players,
                iframe_decay
            ),
        )
        // .add_systems(Last, world::remove_unviewed_chunks) // needs viewer count to update
        // resources
        .insert_resource(ConsoleCommandReceiver { receiver: rx })
        .insert_resource(ServerVersion(VERSION.into()))
        .insert_resource(WorldSeed(SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            / 86_400))
        // events
        .add_event::<ConsoleCommandEvent>()
        .add_event::<ItemAddEvent>()
        .add_event::<SpawnPlayerEvent>()
        // commands
        .add_command::<VersionCommand>()
        .add_command::<PingCommand>()
        .add_command::<GamemodeCommand>()
        .add_command::<TeleportCommand>()
        .add_command::<OpCommand>()
        .add_command::<SetBlockCommand>()
        .add_command::<DebugCommand>()
        .add_command::<GiveCommand>()
        .run();
}

fn core_server_setup() {
    info!("hewwo! running {}.", VERSION);
}

fn setup_core_commands(mut command_scopes: ResMut<CommandScopeRegistry>) {
    // admin
    command_scopes.link("crystal.admin", "crystal.command.version");
    command_scopes.link("crystal.admin", "crystal.command.gamemode");
    command_scopes.link("crystal.admin", "crystal.command.teleport");
    command_scopes.link("crystal.admin", "crystal.command.op");
    command_scopes.link("crystal.admin", "crystal.command.ping");
    command_scopes.link("crystal.admin", "crystal.command.give");
    // NOTE: normal commands tba
}

fn leave_handler(mut clients: Query<&mut Client>, usernames: Query<&Username>, mut removed_clients: RemovedComponents<Client>) {
    for entity in removed_clients.read() {
        let username = usernames.get(entity);
        for mut client in clients.iter_mut() {
            client.send_chat_message(format!("{} left the game", username.unwrap_or(&Username("unknown".to_string()))).color(Color::YELLOW));
        }
        info!("player {} left the game", username.unwrap_or(&Username("unknown".to_string())));
    }
}

fn start_console_input_thread(sender: Sender<String>) {
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in io::BufRead::lines(stdin.lock()) {
            if let Ok(line) = line {
                if sender.send(line).is_err() {
                    error!("[console_thread] channel closed, exiting");
                    break;
                }
            } else {
                error!("[console_thread] error reading line from stdin");
                break;
            }
        }
    });
}

fn poll_console_commands(
    receiver: Res<ConsoleCommandReceiver>,
    mut writer: EventWriter<ConsoleCommandEvent>,
) {
    while let Ok(line) = receiver.receiver.try_recv() {
        writer.send(ConsoleCommandEvent { raw: line });
    }
}

fn init_client(mut clients: Query<&mut Client, Added<Client>>) {
    for mut client in clients.iter_mut() {
        client.set_brand(&format!("Crystal {SIMPLE_VERSION}"));
    }
}
fn init_debug_component(
    clients: Query<Entity, (Added<Client>, Without<DebugStateComponent>)>,
    mut commands: Commands
) {
    for entity in &clients {
        let mut hs = HashSet::new();
        hs.insert(DebugState::Worldgen);
        hs.insert(DebugState::Ticks);
        hs.insert(DebugState::Entities);
        commands.entity(entity).insert(DebugStateComponent {
            enabled: hs
        });
    }
}
