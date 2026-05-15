use tracing::info;
use valence::{command::scopes::CommandScopes, entity::EntityId, op_level::OpLevel, prelude::*, protocol::{VarInt, WritePacket, packets::play::ItemPickupAnimationS2c}};

#[derive(Resource)]
pub struct ServerVersion(pub String);

#[derive(Resource)]
pub struct WorldSeed(pub u64);

pub fn set_op_status(client: &mut Client, username: &Username, which: &mut OpLevel, level: u8, permissions: &mut CommandScopes) {
    which.set(level);
    if level == 4 {
        permissions.add("crystal.admin");
        client.send_chat_message(new_crystal_message(format!("Made {} a server operator", username.0).color(Color::GREEN)));
    } else {
        permissions.remove("crystal.admin");
    }
    info!("{} {}", if level == 4 { "added server operator status for" } else { "revoked operator status for" }, username.0);
    client.trigger_status(match which.get() {
        4 => EntityStatus::SetOpLevel4,
        3 => EntityStatus::SetOpLevel3,
        2 => EntityStatus::SetOpLevel2,
        1 => EntityStatus::SetOpLevel1,
        0 => EntityStatus::SetOpLevel0,
        _ => unreachable!()
    });
}

pub fn new_crystal_message(message: Text) -> Text {
    "[Crystal] ".color(Color::RED) + "".color(Color::GOLD) + message
}

#[derive(Component)]
/// time left (ticks), ran once, collector, count
pub struct ItemPickupTimer(pub i32, pub bool, pub Entity, pub i8);

pub fn delayed_despawn(mut entities: Query<(Entity, &mut ItemPickupTimer)>, mut clients: Query<&mut Client>, mut commands: Commands, entity_ids: Query<&EntityId>) {
    for (ent, mut counter) in entities.iter_mut() {
        if counter.0 <= 0 && counter.1 {
            commands.entity(ent).insert(Despawned);
        } else if counter.0 <= 0 && !counter.1 {
            for mut client in clients.iter_mut() {
                client.write_packet(&ItemPickupAnimationS2c {
                    collected_entity_id: VarInt(**entity_ids.get(ent).unwrap()),
                    collector_entity_id: VarInt(**entity_ids.get(counter.2).unwrap()),
                    pickup_item_count: (counter.3 as i32).into()
                });
            }
            counter.0 = 5;
            counter.1 = true;
        } else {
            counter.0 -= 1;
        }
    }
}
