use valence::{
    client::Client,
    command::handler::CommandResultEvent,
    command_macros::Command,
    keepalive::Ping,
    message::SendMessage,
    prelude::{EventReader, Query, Res},
};

use crate::components::core::{ServerVersion, new_crystal_message};

// #[macro_export]
// macro_rules! command_generator {
//     ($name: ident, ($($scopes: literal),+), ($($paths: literal),+)) => {
//         use valence::command_macros::Command;

//         #[derive(Command, Debug)]
//         #[paths($($paths),*)]
//         #[scopes($($scopes),*)]
//         pub struct $name;
//     };
//     ($name: ident, ($($scopes: literal),+), ($($paths: literal),+), [$($fields: tt)+]) => {
//         use valence::command_macros::Command;

//         #[derive(Command, Debug)]
//         #[paths($($paths),*)]
//         #[scopes($($scopes),*)]
//         pub struct $name {
//             $($fields)*
//         }
//     };
//     ($name: ident, ($($scopes: literal),+), ($($paths: literal),+), {$($fields: tt)+}) => {
//         use valence::command_macros::Command;

//         #[derive(Command, Debug)]
//         #[paths($($paths),*)]
//         #[scopes($($scopes),*)]
//         pub enum $name {
//             $($fields)*
//         }
//     }
// }
// pub macro command_handler_params($name: ident, ($($params: tt),*), $command: ty) {
//     pub fn $name(
//         mut events: EventReader<valence::command::handler::CommandResultEvent<$command>>,
//         mut clients: Query<(Entity, &mut Client, &Username)>,
//         positions: Query<&Position>,
//         $($params)*
//     )
// }
pub macro get_target_selector($selector: expr, $clients: expr, $event: expr, $positions: expr) {
    match $selector {
        valence::command::parsers::EntitySelector::SimpleSelector(ss) => match ss {
            valence::command::parsers::entity_selector::EntitySelectors::AllEntities
            | valence::command::parsers::entity_selector::EntitySelectors::AllPlayers => {
                todo!()
            }
            valence::command::parsers::entity_selector::EntitySelectors::SinglePlayer(name) => {
                let Some(player_entity) = $clients
                    .iter()
                    .find(|(_, _, username)| username.0 == *name)
                    .map(|(entity, ..)| entity)
                else {
                    $clients
                        .get_mut($event.executor)
                        .expect("todo: error handling")
                        .1
                        .send_chat_message("couldn't find player".color(valence::prelude::Color::RED));
                    continue;
                };

                (player_entity, $clients.get_mut(player_entity).expect("todo").1)
            }
            valence::command::parsers::entity_selector::EntitySelectors::SelfPlayer => (
                $event.executor,
                $clients
                    .get_mut($event.executor)
                    .expect("todo: error handling")
                    .1,
            ),
            valence::command::parsers::entity_selector::EntitySelectors::NearestPlayer => {
                let executor_pos = match $positions.get($event.executor) {
                    Ok(pos) => **pos,
                    Err(_) => {
                        $clients.get_mut($event.executor).expect("todo").1
                            .send_chat_message("couldn't find executor".color(valence::prelude::Color::RED));
                        continue;
                    }
                };

                let Some(nearest_entity) = $clients
                    .iter()
                    .filter(|(entity, ..)| *entity != $event.executor)
                    .filter_map(|(entity, ..)| {
                        $positions.get(entity).ok().map(|pos| (entity, pos.distance(executor_pos)))
                    })
                    .min_by(|(_, dist1), (_, dist2)| {
                        dist1.partial_cmp(dist2).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(entity, _)| entity)
                else {
                    $clients.get_mut($event.executor).expect("todo").1
                        .send_chat_message("couldn't find nearest target".color(valence::prelude::Color::RED));
                    continue;
                };

                (nearest_entity, $clients.get_mut(nearest_entity).expect("todo").1)
            }
            valence::command::parsers::entity_selector::EntitySelectors::RandomPlayer => {
                let Some(random_entity) = $clients
                    .iter()
                    .map(|(entity, ..)| entity)
                    .choose(&mut valence::rand::thread_rng())
                else {
                    $clients
                        .get_mut($event.executor)
                        .expect("todo: error handling")
                        .1
                        .send_chat_message("couldn't find random target".color(valence::prelude::Color::RED));
                    continue;
                };

                (random_entity, $clients.get_mut(random_entity).expect("todo").1)
            }
        },
        valence::command::parsers::EntitySelector::ComplexSelector(_, _) => {
            tracing::error!("not implemented");
            continue;
        }
    }
}

#[derive(Command, Clone)]
#[paths("version", "ver")]
#[scopes("crystal.command.version")]
pub struct VersionCommand;

pub fn handle_version_command(
    mut events: EventReader<CommandResultEvent<VersionCommand>>,
    mut clients: Query<&mut Client>,
    version: Res<ServerVersion>,
) {
    for event in events.read() {
        let client = &mut clients.get_mut(event.executor).unwrap();
        client.send_chat_message(new_crystal_message(format!("Running {}", version.0).into()));
    }
}

#[derive(Command, Clone)]
#[paths("ping")]
#[scopes("crystal.command.ping")]
pub struct PingCommand;

pub fn handle_ping_command(
    mut events: EventReader<CommandResultEvent<PingCommand>>,
    mut clients: Query<(&mut Client, &Ping)>,
) {
    for event in events.read() {
        let client = &mut clients.get_mut(event.executor).unwrap();
        client.0.send_chat_message(new_crystal_message(
            format!("Your ping is {}", client.1.0).into(),
        ));
    }
}
