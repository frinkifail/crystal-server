use valence::{
    command::{handler::CommandResultEvent, parsers::{entity_selector::EntitySelectors, EntitySelector}},
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
) -> bool { // Return true on success
    if let Ok(mut components) = clients.get_mut(target) {
        *components.1 = gm; // Mutate GameMode directly
        true
    } else {
        eprintln!("Failed to get gamemode components for entity {:?}", target);
        false
    }
}

// Helper function to send a message to the command executor
fn send_feedback_to_executor(
    message: String,
    is_error: bool,
    clients: &mut Query<(&mut Client, &mut GameMode, &Username, Entity)>,
    executor: Entity,
) {
    if let Ok(mut components) = clients.get_mut(executor) {
        let formatted_message = if is_error {
            format!("[gm] {}", message).color(Color::RED)
        } else {
            format!("[gm] {}", message).color(Color::GOLD)
        };
        components.0.send_chat_message(formatted_message); // Mutate Client
    } else {
        eprintln!("Failed to get client component for executor {:?}", executor);
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
                        format!("Changed your gamemode to {:?}.", game_mode_to_set),
                        false,
                        &mut clients,
                        event.executor,
                    );
                }
                // No explicit else needed, set_player_gamemode prints errors
            }
            // Case 2: Target selector provided
            Some(selector) => match selector {
                EntitySelector::SimpleSelector(simple_selector) => match simple_selector {
                    // --- Subcase: All Players ---
                    EntitySelectors::AllEntities | EntitySelectors::AllPlayers => {
                        // Collect targets first to avoid modifying query while iterating
                        let targets_info: Vec<(Entity, String)> = clients
                            .iter()
                            .map(|(_, _, username, entity)| (entity, username.0.clone()))
                            .collect();

                        let mut success_count = 0;
                        for (target_entity, _) in &targets_info {
                            if set_player_gamemode(*target_entity, &mut clients, game_mode_to_set) {
                                // Optionally notify the target player directly
                                if let Ok(mut target_components) = clients.get_mut(*target_entity) {
                                    target_components.0.send_chat_message(
                                        format!("[gm] Your gamemode was changed to {:?}.", game_mode_to_set)
                                            .color(Color::GOLD),
                                    );
                                }
                                success_count += 1;
                            }
                        }
                        send_feedback_to_executor(
                            format!("Changed gamemode of {} players to {:?}.", success_count, game_mode_to_set),
                            false,
                            &mut clients,
                            event.executor,
                        );
                    }
                    // --- Subcase: Single Player by Name ---
                    EntitySelectors::SinglePlayer(name) => {
                        // Find the target entity ID first without borrowing mutably yet
                        let target_info: Option<(Entity, String)> = clients
                            .iter()
                            .find(|(.., username, _)| username.0 == *name)
                            .map(|(_, _, username, entity)| (entity, username.0.clone()));

                        if let Some((target_entity, target_username)) = target_info {
                            if set_player_gamemode(target_entity, &mut clients, game_mode_to_set) {
                                send_feedback_to_executor(
                                    format!("Changed gamemode of {} to {:?}.", target_username, game_mode_to_set),
                                    false,
                                    &mut clients,
                                    event.executor,
                                );
                            }
                        } else {
                            send_feedback_to_executor(
                                format!("Could not find target: {}", name),
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
                                 format!("Changed your gamemode to {:?}.", game_mode_to_set),
                                 false,
                                 &mut clients,
                                 event.executor,
                             );
                         }
                    }
                     // --- Subcase: Nearest Player ---
                    EntitySelectors::NearestPlayer => {
                         let executor_pos = match positions.get(event.executor) {
                             Ok(pos) => **pos, // Deref Position to get DVec3
                             Err(_) => {
                                 send_feedback_to_executor("Could not get executor position.".to_string(), true, &mut clients, event.executor);
                                 continue; // Skip to next event
                             }
                         };

                         // Find the nearest player without holding a mutable borrow on clients
                         let nearest_target: Option<(Entity, String)> = clients.iter()
                             .filter(|(.., entity)| *entity != event.executor) // Exclude self
                             .filter_map(|(_, _, username, entity)| {
                                 // Get position and calculate distance, filtering out errors
                                 positions.get(entity).ok().map(|pos| (entity, username.0.clone(), pos.distance(executor_pos)))
                             })
                             .min_by(|(_, _, dist1), (_, _, dist2)| {
                                 // Use partial_cmp for robust float comparison
                                 dist1.partial_cmp(dist2).unwrap_or(std::cmp::Ordering::Equal)
                              })
                             .map(|(entity, username, _dist)| (entity, username)); // Discard distance


                         if let Some((target_entity, target_username)) = nearest_target {
                             if set_player_gamemode(target_entity, &mut clients, game_mode_to_set) {
                                send_feedback_to_executor(
                                     format!("Changed gamemode of {} to {:?}.", target_username, game_mode_to_set),
                                     false,
                                     &mut clients,
                                     event.executor,
                                 );
                             }
                         } else {
                             send_feedback_to_executor("Could not find nearest player.".to_string(), true, &mut clients, event.executor);
                         }
                    }
                    // --- Subcase: Random Player ---
                    EntitySelectors::RandomPlayer => {
                        // Choose a random target without borrowing mutably yet
                        let random_target : Option<(Entity, String)> = clients.iter()
                            .map(|(_, _, username, entity)| (entity, username.0.clone())) // Map to needed info
                            .choose(&mut valence::rand::thread_rng()); // Choose one

                        if let Some((target_entity, target_username)) = random_target {
                             if set_player_gamemode(target_entity, &mut clients, game_mode_to_set) {
                                send_feedback_to_executor(
                                     format!("Changed gamemode of {} to {:?}.", target_username, game_mode_to_set),
                                     false,
                                     &mut clients,
                                     event.executor,
                                 );
                             }
                        } else {
                             // This case is unlikely if there's at least one player (the executor)
                             // but good to handle.
                             send_feedback_to_executor("Could not find a random player.".to_string(), true, &mut clients, event.executor);
                        }
                    }
                },
                // --- Subcase: Complex Selector (Not Implemented) ---
                EntitySelector::ComplexSelector(_, _) => {
                    send_feedback_to_executor(
                        "Complex selectors are not implemented.".to_string(),
                        true,
                        &mut clients,
                        event.executor,
                    );
                }
            },
        }
    }
}