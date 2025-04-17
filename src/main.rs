#![allow(clippy::type_complexity)]

use std::{
    io,
    panic::PanicHookInfo,
    thread,
};

// Modules
mod commands;
mod components;
mod world; // Include the new world module
// mod worldtest;

// Use statements for components and commands needed in main
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
// Keep for console input
use valence::{
    command::{AddCommand, CommandScopeRegistry},
    prelude::*,
    rand::seq::SliceRandom,
};

// Constants
const VERSION: &str = "Alpha::0.1";

// --- Panic Handler (Keep as is) ---
fn _crash_handler(info: &PanicHookInfo) {
    error!("[panic] panicked!");
    // ... (rest of crash handler code remains the same)
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
    // Initialize logging (using tracing subscriber is recommended over println)
    // Example using tracing_subscriber:
    // tracing_subscriber::fmt().init();
    // Using basic println for now based on original code:
    info!("Hello! Running {}.", VERSION);

    // panic::set_hook(Box::new(crash_handler));

    // Console input setup (Keep as is)
    let (tx, rx) = unbounded();
    start_console_input_thread(tx);

    // App::new()
    //     .add_plugins(DefaultPlugins)
    //     .add_systems(Startup, worldtest::setup)
    //     .add_systems(
    //         Update,
    //         (
    //             (
    //                 worldtest::init_clients,
    //                 worldtest::remove_unviewed_chunks,
    //                 worldtest::update_client_views,
    //                 worldtest::send_recv_chunks,
    //             )
    //                 .chain(),
    //             despawn_disconnected_clients,
    //         ),
    //     )
    //     .run();

    App::new()
        .add_plugins(DefaultPlugins)
        // -- Startup Systems --
        .add_systems(
            Startup,
            (
                world::setup_world,  // Use world setup from the world module
                setup_core_commands, // Keep command scope setup
            ),
        )
        // -- Update Systems --
        .add_systems(
            Update,
            (
                // World systems (chained as in the terrain example)
                (
                    world::init_clients_world, // Use world-specific client init
                    world::update_client_views,
                    world::send_recv_chunks,
                    // world::remove_unviewed_chunks,
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

fn setup_core_commands(mut command_scopes: ResMut<CommandScopeRegistry>) {
    // Keep command scope linking as is
    command_scopes.link("crystal.admin", "crystal.command.version");
    command_scopes.link("crystal.admin", "crystal.command.gamemode");
    command_scopes.link("crystal.admin", "crystal.command.teleport");
    command_scopes.link("crystal.admin", "crystal.command.op");
}

fn leave_handler(mut removed_clients: RemovedComponents<Client>) {
    // Keep leave handler as is
    for entity in removed_clients.read() {
        info!("Client entity {:?} left the game :(", entity);
    }
}

// --- Console Input (Keep as is) ---
fn start_console_input_thread(sender: Sender<String>) {
    // Keep console thread starter as is
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
    // Keep console poller as is
    while let Ok(line) = receiver.receiver.try_recv() {
        writer.send(ConsoleCommandEvent { raw: line });
    }
}
