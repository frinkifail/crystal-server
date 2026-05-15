use bevy_time::{Real, Time};
use valence::{entity::item::Stack, prelude::*};

use crate::components::items::ItemAddEvent;

#[derive(Component)]
pub struct CanBePickedUpAfter(pub f32 /* seconds */);

const PICKUP_RANGE_SQUARED: f64 = 1.5 * 1.5;

pub fn item_pickup_system(
    mut item_query: Query<(Entity, &Position, &Stack, &mut CanBePickedUpAfter)>,
    player_query: Query<(Entity, &Position)>,
    time: Res<Time<Real>>,
    mut events: EventWriter<ItemAddEvent>
) {
    for (item_entity, item_pos, item_stack_comp, mut pickup_timer) in item_query.iter_mut() {
        pickup_timer.0 -= time.delta_seconds();

        if pickup_timer.0 > 0.0 { continue }
        for (entity, player_pos) in player_query.iter() {
            if item_pos.0.distance_squared(player_pos.0) > PICKUP_RANGE_SQUARED { continue }

            events.send(ItemAddEvent {
                player: entity,
                item: item_stack_comp.item,
                count: item_stack_comp.count,
                source: Some(item_entity)
            });
        }
    }
}
