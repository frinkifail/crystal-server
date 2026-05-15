use std::collections::HashSet;
use bevy_time::{Real, Time};
use tracing::error;
use valence::{client::Client, command::handler::CommandResultEvent, command_macros::Command, ecs::entity::Entities, entity::Velocity, prelude::*};

use crate::world::WorldState;

#[derive(Eq, PartialEq, Hash)]
pub enum DebugState {
    Worldgen,
    Ticks,
    Entities,
    Velocity
}

#[derive(Component)]
pub struct DebugStateComponent {
    pub enabled: HashSet<DebugState>,
}

#[derive(Command, Clone, Debug)]
#[paths("debug")]
pub enum DebugCommand {
    #[paths("worldgen {toggle}")]
    Worldgen { toggle: bool },
    #[paths("ticks {toggle}")]
    Ticks { toggle: bool },
    #[paths("entities {toggle}")]
    Entities { toggle: bool },
    #[paths("vel {toggle}")]
    Velocity { toggle: bool }
}

pub fn tick_debug(
    mut clients: Query<(&mut Client, &Velocity, &mut DebugStateComponent)>,
    state: Res<WorldState>,
    time: Res<Time<Real>>,
    server: Res<Server>,
    entities: &Entities
) {
    for (mut client, vel, debug) in clients.iter_mut() {
        let mut strings: Vec<String> = vec![];
        if debug.enabled.contains(&DebugState::Worldgen) {
            strings.push(format!("pending: {} | waiting queue: {} | sending queue: {}", state.pending_chunks.len(), state.receiver.len(), state.sender.len()));
        }
        if debug.enabled.contains(&DebugState::Ticks) {
            strings.push(format!("dt: {:.1}ms | tps: {:.2} | target tps: {}", time.delta().as_millis_f32(), 1.0/time.delta_seconds(), server.tick_rate()));
        }
        if debug.enabled.contains(&DebugState::Entities) {
            strings.push(format!("entities: {}", entities.len()));
        }
        if debug.enabled.contains(&DebugState::Velocity) {
            strings.push(format!("vel: {}", vel.0));
        }
        client.send_action_bar_message(strings.join(" | "));
    }
}

pub fn handle_debug_command(
    mut events: EventReader<CommandResultEvent<DebugCommand>>,
    mut clients: Query<(&mut Client, &mut DebugStateComponent)>
) {
    for event in events.read() {
        let Ok(mut components) = clients.get_mut(event.executor) else {
            error!("failed to get client");
            continue;
        };
        let toggled = match &event.result {
            DebugCommand::Worldgen { toggle } => {
                if *toggle {
                    components.1.enabled.insert(DebugState::Worldgen);
                } else {
                    components.1.enabled.remove(&DebugState::Worldgen);
                }
                *toggle
            }
            DebugCommand::Ticks { toggle } => {
                if *toggle {
                    components.1.enabled.insert(DebugState::Ticks);
                } else {
                    components.1.enabled.remove(&DebugState::Ticks);
                }
                *toggle
            }
            DebugCommand::Entities { toggle } => {
                if *toggle {
                    components.1.enabled.insert(DebugState::Entities);
                } else {
                    components.1.enabled.remove(&DebugState::Entities);
                }
                *toggle
            }
            DebugCommand::Velocity { toggle } => {
                if *toggle {
                    components.1.enabled.insert(DebugState::Velocity);
                } else {
                    components.1.enabled.remove(&DebugState::Velocity);
                }
                *toggle
            }
        };
        components.0.send_chat_message("[debug] set ".color(Color::GOLD) + format!("{:?}", event.result).split("{").next().unwrap().trim().to_string().color(Color::RED) + " to ".color(Color::GOLD) + toggled.into_text().color(Color::RED));
    }
}
