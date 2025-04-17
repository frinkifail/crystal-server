use crossbeam_channel::Receiver;
use tracing::{error, info};
use valence::{client::DisconnectClient, command::scopes::CommandScopes, op_level::OpLevel, prelude::*};

use super::core::set_op_status;

#[derive(Resource)]
pub struct ConsoleCommandReceiver {
    pub receiver: Receiver<String>
}

#[derive(Event)]
pub struct ConsoleCommandEvent {
    pub raw: String,
}

pub fn handle_console_command(
    // mut world: ResMut<World>,
    mut commands: Commands,
    mut events: EventReader<ConsoleCommandEvent>,
    mut clients: Query<(Entity, &mut Client, &mut Username, &mut OpLevel, &mut CommandScopes), With<Client>>
    // mut clients: Query<&mut Client>,
) {
    for event in events.read() {
        let cmd = event.raw.trim();
        let mut split = cmd.split_ascii_whitespace();
        let name = split.next().unwrap_or("");
        let args: Vec<&str> = split.collect();

        match name {
            "stop" => {
                info!("Stopping server...");
                for client in clients.iter() {
                    commands.add(DisconnectClient { client: client.0, reason: "Server closed".into() });
                }
                std::process::exit(0);
            },
            "players" => {
                info!("Online players: {}", clients.iter().count());
            },
            "op" => {
                let player_name = args.get(0).unwrap_or(&"");
                for (_, mut client, username, mut op_level, mut permissions) in clients.iter_mut() {
                    if username.0 == player_name.to_owned() {
                        set_op_status(&mut client, &username, &mut op_level, None, &mut permissions);
                    }
                }
            },
            _ => error!("unknown command")
        }
    }
}
