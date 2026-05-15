use valence::{command::{handler::CommandResultEvent, parsers::EntitySelector}, command_macros::Command, entity::item::{ItemEntityBundle, Stack}, prelude::*};
use valence::rand::seq::IteratorRandom;
use crate::{commands::core::get_target_selector, components::{core::ItemPickupTimer, items::ItemAddEvent}};

#[derive(Command, Debug)]
#[paths("give {target} {item} {count?}")]
#[scopes("crystal.command.give")]
pub struct GiveCommand {
    target: EntitySelector,
    item: String,
    count: Option<i32>
}

pub fn handle_give_command(
    mut events: EventReader<CommandResultEvent<GiveCommand>>,
    mut clients: Query<(Entity, &mut Client, &Username)>,
    positions: Query<&Position, With<Client>>,
    mut item_events: EventWriter<ItemAddEvent>,
    mut em: ResMut<EntityManager>,
    mut commands: Commands,
    entity_layers: Query<&EntityLayerId, With<Client>>
) {
    for event in events.read() {
        let (ent, mut client) = get_target_selector!(event.result.target.clone(), clients, event, positions);

        let Some(item_kind) = ItemKind::from_str(&event.result.item) else {
            client.send_chat_message("[give] invalid item id".color(Color::RED));
            continue;
        };

        let count = event.result.count.unwrap_or(1).clamp(1, 127) as i8;

        let item = ItemStack::new(item_kind, event.result.count.unwrap_or(1) as i8, None);
        let id = em.next_id();

        let Ok(entity_layer) = entity_layers.get(ent) else {
            client.send_chat_message("[give] invalid entity layer".color(Color::RED));
            continue;
        };

        let mut item_entity = commands.spawn(ItemEntityBundle {
            layer: *entity_layer,
            item_stack: Stack(item.clone()),
            position: positions.get(ent).unwrap().clone(),
            id,
            ..Default::default()
        });

        item_entity.insert(ItemPickupTimer(3, false, ent, event.result.count.unwrap_or(1) as i8));

        item_events.send(ItemAddEvent {
            player: ent,
            item: item_kind,
            count,
            source: None // or spawn the entity here if you want anim
        });
    }
}

// pub fn handle_give_command(
//     mut events: EventReader<CommandResultEvent<GiveCommand>>,
//     mut clients: Query<(Entity, &mut Client, &Username)>,
//     positions: Query<&Position, With<Client>>,
//     mut inventories: Query<&mut Inventory, With<Client>>,
//     mut commands: Commands,
//     entity_layers: Query<&EntityLayerId, With<Client>>,
//     mut em: ResMut<EntityManager>
// ) {
//     for event in events.read() {
//         let (ent, mut client) = get_target_selector!(event.result.target.clone(), clients, event, positions);
//         let Ok(mut inv) = inventories.get_mut(ent) else {
//             error!("failed to find inventory for {}", clients.get(ent).unwrap().2.0);
//             continue;
//         };

//         let Some(slot) = inv.first_empty_slot_in(9..45) else {
//             client.send_chat_message("[give] your inventory is full".color(Color::RED));
//             continue;
//         };

//         let Some(item_kind) = ItemKind::from_str(&event.result.item) else {
//             client.send_chat_message("[give] invalid item id".color(Color::RED));
//             continue;
//         };

//         let Ok(entity_layer) = entity_layers.get(ent) else {
//             client.send_chat_message("[give] invalid entity layer".color(Color::RED));
//             continue;
//         };

//         let item = ItemStack::new(item_kind, event.result.count.unwrap_or(1) as i8, None);
//         let id = em.next_id();

//         let mut item_entity = commands.spawn(ItemEntityBundle {
//             layer: *entity_layer,
//             item_stack: Stack(item.clone()),
//             position: positions.get(ent).unwrap().clone(),
//             id,
//             ..Default::default()
//         });

//         item_entity.insert(ItemPickupTimer(3, false, ent, event.result.count.unwrap_or(1) as i8));

//         inv.set_slot(slot, item);
//     }
// }
