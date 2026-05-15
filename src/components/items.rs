use valence::{entity::{EntityId, item::{ItemEntity, Stack}}, prelude::*, protocol::{VarInt, WritePacket, packets::play::ItemPickupAnimationS2c}};
use tracing::warn;

use crate::components::core::ItemPickupTimer;

#[derive(Event)]
pub struct ItemAddEvent {
    pub player: Entity,
    pub item: ItemKind,
    pub count: i8,
    pub source: Option<Entity>
}

pub fn item_add_handler(mut events: EventReader<ItemAddEvent>, mut player_query: Query<(&mut Inventory, &mut Client, &EntityId)>, mut commands: Commands, mut item_query: Query<(&EntityId, &mut Stack), With<ItemEntity>>) {
    for event in events.read() {
        let Ok(queried) = player_query.get_mut(event.player) else { continue };
        let (mut inventory, mut client, player_id) = queried;
        let item_id = event.item;
        let stack_count = event.count;

        let slot_index =
            if let Some(i) = inventory.first_slot_with_item(item_id, 64) {
                i
            } else if let Some(i) = inventory.first_empty_slot_in(36..45) {
                i
            } else if let Some(i) = inventory.first_empty_slot_in(9..36) {
                i
            } else {
                continue;
            };

        let initial_count = inventory.slot(slot_index).count;
        let max_stack: i8 = 64;
        let space_left = max_stack.saturating_sub(initial_count);

        let to_add = stack_count.min(space_left);
        let remainder_count = stack_count - to_add;

        if to_add > 0 {
            inventory.set_slot(
                slot_index,
                ItemStack {
                    item: item_id,
                    count: initial_count + to_add,
                    nbt: None,
                },
            );

            if let Some(source) = event.source {
                if let Ok(mut item_query) = item_query.get_mut(source) {
                    item_query.1.0.count = remainder_count;

                    commands.entity(source).insert(ItemPickupTimer(2, false, event.player, stack_count));
                    client.write_packet(&ItemPickupAnimationS2c {
                        collected_entity_id: VarInt(**item_query.0),
                        collector_entity_id: VarInt(**player_id),
                        pickup_item_count: (stack_count as i32).into()
                    });
                } else {
                    warn!("Item query not found for this item.")
                }
            }
        }
    }
}
