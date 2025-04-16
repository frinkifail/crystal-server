use valence::{client::Client, command::handler::CommandResultEvent, command_macros::Command, message::SendMessage, prelude::{EventReader, Query, Res}};

use crate::components::core::{new_crystal_message, ServerVersion};

#[derive(Command, Clone)]
#[paths("version", "ver")]
#[scopes("crystal.command.version")]
pub struct VersionCommand;

pub fn handle_version_command(mut events: EventReader<CommandResultEvent<VersionCommand>>, mut clients: Query<&mut Client>, version: Res<ServerVersion>) {
    for event in events.read() {
        let client = &mut clients.get_mut(event.executor).unwrap();
        client.send_chat_message(new_crystal_message(format!("Running {}", version.0).into()));
    }
}
