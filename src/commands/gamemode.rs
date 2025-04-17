use tracing::error;
use valence::{
    command::{
        handler::CommandResultEvent,
        parsers::{EntitySelector, entity_selector::EntitySelectors},
    },
    command_macros::Command,
    prelude::*,
    rand::seq::IteratorRandom,
};

#[derive(Command, Debug, Clone)]
#[paths("gamemode", "gm")]
#[scopes("crystal.command.gamemode")]
pub enum GamemodeCommand {
    #[paths("survival {target?}", "{/} gms {target?}")]
    Survival { target: Option<EntitySelector> },
    #[paths("creative {target?}", "{/} gmc {target?}")]
    Creative { target: Option<EntitySelector> },
    #[paths("adventure {target?}", "{/} gma {target?}")]
    Adventure { target: Option<EntitySelector> },
    #[paths("spectator {target?}", "{/} gmsp {target?}")]
    Spectator { target: Option<EntitySelector> },
}

// Helper function to set gamemode for a single target
fn set_player_gamemode(
    target: Entity,
    clients: &mut Query<(&mut Client, &mut GameMode, &Username, Entity)>,
    gm: GameMode,
) -> bool {
    // Return true on success
    if let Ok(mut components) = clients.get_mut(target) {
        *components.1 = gm; // Mutate GameMode directly
        true
    } else {
        error!("failed to get gamemode components for entity {:?}", target);
        false
    }
}

// Helper function to send a message to the command executor
fn send_feedback_to_executor(
    message: Text,
    is_error: bool,
    clients: &mut Query<(&mut Client, &mut GameMode, &Username, Entity)>,
    executor: Entity,
) {
    if let Ok(mut components) = clients.get_mut(executor) {
        let formatted_message = if is_error {
            "[gm] ".color(Color::RED) + message.color(Color::RED)
        } else {
            "[gm] ".color(Color::GOLD) + message
        };
        components.0.send_chat_message(formatted_message); // Mutate Client
    } else {
        error!("failed to get client component for executor {:?}", executor);
    }
}

// Helper function to format gamemode change messages
fn format_gamemode_message(
    prefix: &str,
    target: Option<&str>,
    gamemode: GameMode,
) -> Text {
    let gamemode_string = format!("{:?}", gamemode).color(Color::RED);
    let prefix_colored = prefix.to_string().color(Color::GOLD);
    // let target_string = 

    if let Some(target_name) = target.clone() {
        prefix_colored
            + " "
            + target_name.to_string().color(Color::RED)
            + " "
            + "gamemode to".color(Color::GOLD)
            + " "
            + gamemode_string
            + "."
    } else {
        prefix_colored
            + " "
            + "your gamemode to".color(Color::GOLD)
            + " "
            + gamemode_string
            + "."
    }
}

pub fn handle_gamemode_command(
    mut events: EventReader<CommandResultEvent<GamemodeCommand>>,
    mut clients: Query<(&mut Client, &mut GameMode, &Username, Entity)>, // Keep the query mutable here
    positions: Query<&Position>,
) {
    for event in events.read() {
        let game_mode_to_set = match &event.result {
            GamemodeCommand::Survival { .. } => GameMode::Survival,
            GamemodeCommand::Creative { .. } => GameMode::Creative,
            GamemodeCommand::Adventure { .. } => GameMode::Adventure,
            GamemodeCommand::Spectator { .. } => GameMode::Spectator,
        };

        let selector = match &event.result {
            GamemodeCommand::Survival { target }
            | GamemodeCommand::Creative { target }
            | GamemodeCommand::Adventure { target }
            | GamemodeCommand::Spectator { target } => target.clone(),
        };

        // --- Start of Match ---
        match selector {
            // Case 1: No target selector provided (apply to executor)
            None => {
                let target = event.executor;
                if set_player_gamemode(target, &mut clients, game_mode_to_set) {
                    send_feedback_to_executor(
                        format_gamemode_message("changed", None, game_mode_to_set),
                        false,
                        &mut clients,
                        event.executor,
                    );
                }
            }
            // Case 2: Target selector provided
            Some(selector) => match selector {
                EntitySelector::SimpleSelector(simple_selector) => match simple_selector {
                    // --- Subcase: All Players ---
                    EntitySelectors::AllEntities | EntitySelectors::AllPlayers => {
                        let targets_info: Vec<(Entity, String)> = clients
                            .iter()
                            .map(|(_, _, username, entity)| (entity, username.0.clone()))
                            .collect();

                        let mut success_count = 0;
                        for (target_entity, _) in &targets_info {
                            if set_player_gamemode(*target_entity, &mut clients, game_mode_to_set) {
                                if let Ok(mut target_components) = clients.get_mut(*target_entity) {
                                    target_components.0.send_chat_message(
                                        format_gamemode_message(
                                            "your gamemode was changed to",
                                            None,
                                            game_mode_to_set,
                                        ),
                                    );
                                }
                                success_count += 1;
                            }
                        }
                        send_feedback_to_executor(
                            format!(
                                "[gm] changed gamemode of {} players to {:?}.",
                                success_count, game_mode_to_set
                            ).color(Color::GOLD),
                            false,
                            &mut clients,
                            event.executor,
                        );
                    }
                    // --- Subcase: Single Player by Name ---
                    EntitySelectors::SinglePlayer(name) => {
                        let target_info: Option<(Entity, String)> = clients
                            .iter()
                            .find(|(.., username, _)| username.0 == *name)
                            .map(|(_, _, username, entity)| (entity, username.0.clone()));

                        if let Some((target_entity, target_username)) = target_info {
                            if set_player_gamemode(target_entity, &mut clients, game_mode_to_set) {
                                send_feedback_to_executor(
                                    format_gamemode_message(
                                        "changed",
                                        Some(&target_username),
                                        game_mode_to_set,
                                    ),
                                    false,
                                    &mut clients,
                                    event.executor,
                                );
                            }
                        } else {
                            send_feedback_to_executor(
                                format!("could not find target: {}", name).into(),
                                true,
                                &mut clients,
                                event.executor,
                            );
                        }
                    }
                    // --- Subcase: Executor Self ---
                    EntitySelectors::SelfPlayer => {
                        let target = event.executor;
                        if set_player_gamemode(target, &mut clients, game_mode_to_set) {
                            send_feedback_to_executor(
                                format_gamemode_message("changed", None, game_mode_to_set),
                                false,
                                &mut clients,
                                event.executor,
                            );
                        }
                    }
                    // --- Subcase: Nearest Player ---
                    EntitySelectors::NearestPlayer => {
                        let executor_pos = match positions.get(event.executor) {
                            Ok(pos) => **pos,
                            Err(_) => {
                                send_feedback_to_executor(
                                    "could not get executor position.".into(),
                                    true,
                                    &mut clients,
                                    event.executor,
                                );
                                continue;
                            }
                        };

                        let nearest_target: Option<(Entity, String)> = clients
                            .iter()
                            .filter(|(.., entity)| *entity != event.executor)
                            .filter_map(|(_, _, username, entity)| {
                                positions.get(entity).ok().map(|pos| {
                                    (entity, username.0.clone(), pos.distance(executor_pos))
                                })
                            })
                            .min_by(|(_, _, dist1), (_, _, dist2)| {
                                dist1
                                    .partial_cmp(dist2)
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            })
                            .map(|(entity, username, _dist)| (entity, username));

                        if let Some((target_entity, target_username)) = nearest_target {
                            if set_player_gamemode(target_entity, &mut clients, game_mode_to_set) {
                                send_feedback_to_executor(
                                    format_gamemode_message(
                                        "changed",
                                        Some(&target_username),
                                        game_mode_to_set,
                                    ),
                                    false,
                                    &mut clients,
                                    event.executor,
                                );
                            }
                        } else {
                            send_feedback_to_executor(
                                "could not find nearest player.".into(),
                                true,
                                &mut clients,
                                event.executor,
                            );
                        }
                    }
                    // --- Subcase: Random Player ---
                    EntitySelectors::RandomPlayer => {
                        let random_target: Option<(Entity, String)> = clients
                            .iter()
                            .map(|(_, _, username, entity)| (entity, username.0.clone()))
                            .choose(&mut valence::rand::thread_rng());

                        if let Some((target_entity, target_username)) = random_target {
                            if set_player_gamemode(target_entity, &mut clients, game_mode_to_set) {
                                send_feedback_to_executor(
                                    format_gamemode_message(
                                        "changed",
                                        Some(&target_username),
                                        game_mode_to_set,
                                    ),
                                    false,
                                    &mut clients,
                                    event.executor,
                                );
                            }
                        } else {
                            send_feedback_to_executor(
                                "could not find a random player.".into(),
                                true,
                                &mut clients,
                                event.executor,
                            );
                        }
                    }
                },
                // --- Subcase: Complex Selector (Not Implemented) ---
                EntitySelector::ComplexSelector(_, _) => {
                    send_feedback_to_executor(
                        "complex selectors are not implemented.".into(),
                        true,
                        &mut clients,
                        event.executor,
                    );
                }
            },
        }
    }
}
