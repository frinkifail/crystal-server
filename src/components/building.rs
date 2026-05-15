use valence::{entity::{Velocity, item::{ItemEntityBundle, Stack}}, interact_block::InteractBlockEvent, inventory::HeldItem, prelude::*, rand::{Rng, thread_rng}};
use tracing::error;
use crate::runtime_data::HasExtraBlockData;

use super::itempickup::CanBePickedUpAfter;

pub fn digging(
    mut commands: Commands,
    mut clients: Query<(&GameMode, &mut Client)>,
    mut layers: Query<&mut ChunkLayer>,
    mut events: EventReader<DiggingEvent>,
    entity_layers: Query<&EntityLayerId>,
    mut em: ResMut<EntityManager>
) {
    // NOTE: use `layers.get(event.client)` inside [1] when adding other chunk layers
    let mut layer = layers.single_mut();

    for event in events.read() {
        let Ok((game_mode, mut client)) = clients.get_mut(event.client) else {
            continue;
        };

        // [1]
        let entity_layer = entity_layers.get(event.client);

        let Some(blockref) = layer.block(event.position) else { error!("digging... nothing??"); continue; };
        let blockkind = blockref.state.to_kind();

        if (*game_mode == GameMode::Creative && event.state == DiggingState::Start)
            || (*game_mode == GameMode::Survival && (event.state == DiggingState::Stop || blockkind.hardness() <= 0.0))
        {
            layer.set_block(event.position, BlockState::AIR);
            if let Ok(entity_layer) = entity_layer && *game_mode == GameMode::Survival {
                let id = em.next_id();

                if let Some(itemkind) = blockkind.random_drop() {
                    commands.spawn(ItemEntityBundle {
                        layer: *entity_layer,
                        item_stack: Stack(ItemStack::new(itemkind, 1, None)),
                        position: Position(DVec3::new(
                            event.position.x as f64 + 0.5,
                            event.position.y as f64 + 0.2,
                            event.position.z as f64 + 0.5
                        )),
                        velocity: Velocity(Vec3::new(thread_rng().gen_range(-1.0..1.0), 4.4, thread_rng().gen_range(-1.0..1.0))),
                        id,
                        ..Default::default()
                    })
                        .insert(CanBePickedUpAfter(0.5));
                }
            } else if let Err(ref error) = entity_layer {
                client.send_action_bar_message(format!("failed to spawn item. {}", error).color(Color::RED));
            }
        }
    }
}

pub fn place_blocks(
    mut clients: Query<(&mut Inventory, &GameMode, &HeldItem)>,
    mut layers: Query<&mut ChunkLayer>,
    mut events: EventReader<InteractBlockEvent>,
) {
    let mut layer = layers.single_mut();

    for event in events.read() {
        let Ok((mut inventory, game_mode, held)) = clients.get_mut(event.client) else {
            continue;
        };
        if event.hand != Hand::Main {
            continue;
        }

        // get the held item
        let slot_id = held.slot();
        let stack = inventory.slot(slot_id);
        if stack.is_empty() {
            // no item in the slot
            continue;
        };

        let Some(block_kind) = BlockKind::from_item_kind(stack.item) else {
            // can't place this item as a block
            continue;
        };

        if *game_mode == GameMode::Survival {
            // check if the player has the item in their inventory and remove
            // it.
            if stack.count > 1 {
                let amount = stack.count - 1;
                inventory.set_slot_amount(slot_id, amount);
            } else {
                inventory.set_slot(slot_id, ItemStack::EMPTY);
            }
        }
        let real_pos = event.position.get_in_direction(event.face);
        let state = block_kind.to_state().set(
            PropName::Axis,
            match event.face {
                Direction::Down | Direction::Up => PropValue::Y,
                Direction::North | Direction::South => PropValue::Z,
                Direction::West | Direction::East => PropValue::X,
            },
        );
        layer.set_block(real_pos, state);
    }
}
