use valence::{command::{handler::CommandResultEvent, parsers::{entity_selector::EntitySelectors, EntitySelector}, scopes::CommandScopes}, command_macros::Command, op_level::OpLevel, prelude::*};

use crate::components::core::set_op_status;

#[derive(Command, Debug, Clone)]
#[paths("op {target?}")]
#[scopes("crystal.command.op")]
pub struct OpCommand {
    target: Option<EntitySelector>
}

fn send_message(client: &mut Client, message: &str, color: Color) {
    client.send_chat_message(message.to_string().color(color));
}

pub fn handle_op_command(
    mut events: EventReader<CommandResultEvent<OpCommand>>,
    mut clients: Query<(&mut Client, &Username, Entity, &mut OpLevel, &mut CommandScopes)>,
) {
    for event in events.read() {
        let selector = &event.result.target;

        match selector {
            None => {
                let (mut client, username, _, mut oplevel, mut permissions) = clients.get_mut(event.executor).unwrap();
                set_op_status(&mut client, &username, &mut oplevel, Some(true), &mut permissions);
            }
            Some(selector) => match selector {
                EntitySelector::SimpleSelector(selector) => match selector {
                    EntitySelectors::AllEntities => {
                        let client = &mut clients.get_mut(event.executor).unwrap().0;
                        send_message(client, "[op] can't op entities", Color::RED);
                    }
                    EntitySelectors::SinglePlayer(name) => {
                        let target = clients
                            .iter_mut()
                            .find(|(_, username, _, ..)| username.0 == *name)
                            .map(|(_, _, target, ..)| target);

                        let client = &mut clients.get_mut(event.executor).unwrap().0;
                        match target {
                            None => send_message(client, &format!("[op] could not find target: {name}"), Color::RED),
                            Some(_) => send_message(client, &format!("[op] successfully opped {name}"), Color::GREEN),
                        }
                    }
                    EntitySelectors::AllPlayers => {
                        for (mut client, username, _, mut oplevel, mut permissions) in &mut clients.iter_mut() {
                            set_op_status(&mut client, &username, &mut oplevel, Some(true), &mut permissions);
                        }
                        let clientexec = &mut clients.get_mut(event.executor).unwrap().0;
                        send_message(clientexec, "[op] successfully opped everyone", Color::GREEN);
                    }
                    EntitySelectors::SelfPlayer => {
                        let client = &mut clients.get_mut(event.executor).unwrap().0;
                        send_message(client, "[op] can't op yourself", Color::RED);
                    }
                    EntitySelectors::NearestPlayer | EntitySelectors::RandomPlayer => {
                        let client = &mut clients.get_mut(event.executor).unwrap().0;
                        send_message(client, "[op] work in progress", Color::RED);
                    }
                },
                EntitySelector::ComplexSelector(_, _) => {
                    let client = &mut clients.get_mut(event.executor).unwrap().0;
                    send_message(client, "[op] complex selector not implemented", Color::RED);
                }
            },
        }
    }
}
