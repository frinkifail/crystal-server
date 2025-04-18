#![allow(clippy::type_complexity)]
#![feature(let_chains)]

use std::{
    io,
    panic::PanicHookInfo,
    thread,
};

// Modules
mod commands;
mod components;
mod world;

use commands::{
    core::{VersionCommand, handle_version_command},
    gamemode::{GamemodeCommand, handle_gamemode_command},
    op::{OpCommand, handle_op_command},
    teleport::{TeleportCommand, handle_teleport_command},
};
use components::{
    building::{digging, place_blocks}, chat::chat_message_event, console::{handle_console_command, ConsoleCommandEvent, ConsoleCommandReceiver}, core::ServerVersion
};
use crossbeam_channel::{Sender, unbounded}; use tracing::{error, info};
use valence::{
    command::{AddCommand, CommandScopeRegistry}, prelude::*, rand::seq::SliceRandom
};

// Constants
const VERSION: &str = "Alpha(dev)::0.4 (item)";

// --- Panic Handler ---
fn _crash_handler(info: &PanicHookInfo) {
    error!("[panic] panicked!");
    let premessage = [
        "&crystal::CrashLog",
        ">> crystal crash log",
        "Query<&CrashLog>",
        "*crashlog",
    ];
    let comments = [
        "not my fault",
        "cat ate my homework",
        "This is quite perplexing indeed.",
        "working my ass",
        "skill issue",
    ];
    let mut rng = valence::rand::thread_rng();
    let location = info.location().unwrap();
    let panic_text = format!(
        "{}\n// {}\n{:?}: file '{}' line {}",
        premessage.choose(&mut rng).unwrap_or(&"crashed 3:"),
        comments.choose(&mut rng).unwrap_or(&"No comment."),
        info.payload(),
        location.file(),
        location.line()
    );
    error!("{}", panic_text);
}

// --- Main Function ---
fn main() {
    // tracing_subscriber::fmt().init();

    // Hook the panic for a more friendly crash message when in release mode :D
    #[cfg(not(debug_assertions))]
    panic::set_hook(Box::new(crash_handler));

    // Setup console commands
    let (tx, rx) = unbounded();
    start_console_input_thread(tx);

    App::new()
        .add_plugins(DefaultPlugins)
        // -- Startup Systems --
        .add_systems(
            Startup,
            (
                core_server_setup,
                world::setup_world,
                setup_core_commands,
            ),
        )
        // -- Update Systems --
        .add_systems(
            Update,
            (
                // World systems
                (
                    world::init_clients_world,
                    world::update_client_views,
                    world::send_recv_chunks,
                    // "remove unviewed chunks" is run later.
                )
                    .chain(),
                // Core systems
                despawn_disconnected_clients,
                leave_handler,
                chat_message_event,
                digging,
                place_blocks,
                // Console systems
                poll_console_commands,
                handle_console_command, // Ensure this is defined in components/console.rs
                // Command handlers (from commands module)
                handle_version_command,
                handle_teleport_command,
                handle_gamemode_command,
                handle_op_command,
            ),
        )
        // Must be run in `Last` because viewer_count needs to update first.
        .add_systems(Last, world::remove_unviewed_chunks)
        // -- Resources --
        .insert_resource(ConsoleCommandReceiver { receiver: rx })
        .insert_resource(ServerVersion(VERSION.into()))
        // -- Events --
        .add_event::<ConsoleCommandEvent>()
        // -- Commands --
        .add_command::<VersionCommand>()
        .add_command::<GamemodeCommand>()
        .add_command::<TeleportCommand>()
        .add_command::<OpCommand>()
        .run();
}

// --- Core Setup/Handlers in Main ---

fn core_server_setup() {
    info!("Hello! Running {}.", VERSION);
}

fn setup_core_commands(mut command_scopes: ResMut<CommandScopeRegistry>) {
    // --- Admin commands ---
    command_scopes.link("crystal.admin", "crystal.command.version");
    command_scopes.link("crystal.admin", "crystal.command.gamemode");
    command_scopes.link("crystal.admin", "crystal.command.teleport");
    command_scopes.link("crystal.admin", "crystal.command.op");
    // NOTE: Normal commands TBA
}

fn leave_handler(mut removed_clients: RemovedComponents<Client>) {
    // TODO: store player name before getting removed
    for entity in removed_clients.read() {
        info!("Client entity {:?} left the game :(", entity);
    }
}

// --- Console Input ---
fn start_console_input_thread(sender: Sender<String>) {
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in io::BufRead::lines(stdin.lock()) {
            if let Ok(line) = line {
                if sender.send(line).is_err() {
                    error!("[console_thread] Main thread channel closed, exiting.");
                    break;
                }
            } else {
                error!("[console_thread] Error reading line from stdin.");
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
