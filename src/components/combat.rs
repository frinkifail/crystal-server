use valence::{command::scopes::CommandScopes, ecs::query::QueryData, entity::{EntityId, EntityStatuses, Velocity, living::Health, player::Saturation}, math::Vec3Swizzles, op_level::OpLevel, prelude::*, protocol::{Sound, VarInt, WritePacket, packets::play::{EntityDamageS2c, PlayerRespawnS2c}, sound::SoundCategory}, registry::TagsRegistry, status::RequestRespawnEvent};
use tracing::info;
use crate::{components::core::set_op_status, world::SPAWN_POS};

#[derive(Component)]
pub struct CombatState {
    pub last_attacked_tick: i64,
    pub iframe_timer: i64,
    pub last_attacked_damage: f32
}

const IFRAME_TIMER: i64 = 10; // ticks

pub fn iframe_decay(mut combat_states: Query<&mut CombatState>) {
    for mut cs in combat_states.iter_mut() {
        if cs.iframe_timer <= 0 { cs.last_attacked_damage = 0.0; continue }
        cs.iframe_timer -= 1;
    }
}

#[derive(QueryData)]
#[query_data(mutable)]
pub struct CombatQuery {
    entity: Entity,
    client: Option<&'static mut Client>,
    position: &'static Position,
    velocity: &'static mut Velocity,
    health: &'static mut Health,
    combat_state: &'static mut CombatState,
    status: &'static mut EntityStatuses,
    eid: &'static EntityId
}

pub fn attacking(
    mut events: EventReader<InteractEntityEvent>,
    mut entities: Query<CombatQuery>,
    server: Res<Server>
) {
    for event in events.read() {
        if event.interact != EntityInteraction::Attack {
            continue;
        }
        let Ok([mut attacker, mut victim]) = entities.get_many_mut([event.client, event.entity]) else { continue };

        let attack_cooldown = 5.0;
        let ticks_since_last_attack = (server.current_tick() - attacker.combat_state.last_attacked_tick) as f32;
        let attack_strength = (ticks_since_last_attack / attack_cooldown).min(1.0);
        let attacker_falling = attacker.velocity.y < 0.0;
        let damage = (1.0 * (0.2 + 0.8 * attack_strength.powi(2))) * if attacker_falling { 1.5 } else { 1.0 };

        if victim.combat_state.iframe_timer > 0 && victim.combat_state.last_attacked_damage >= damage {
            continue;
        }

        **victim.health -= damage;
        victim.combat_state.last_attacked_damage = damage;
        victim.combat_state.iframe_timer = IFRAME_TIMER;

        attacker.combat_state.last_attacked_tick = server.current_tick();

        let victim_pos = victim.position.xz();
        let attacker_pos = attacker.position.xz();

        let dir = (victim_pos - attacker_pos).normalize().as_vec2();

        let knockback_xz = 8.0;
        let knockback_y = 6.432;

        **victim.velocity = Vec3::from_array([dir.x * knockback_xz, knockback_y, dir.y * knockback_xz]);

        let client = attacker.client.as_mut().unwrap();

        victim.status.trigger(EntityStatus::PlayAttackSound);
        client.trigger_status(EntityStatus::PlayAttackSound);
        client.play_sound(
            Sound::EntityPlayerHurt,
            SoundCategory::Player,
            attacker.position.as_vec3(),
            1.0,
            1.0,
        );
        if attacker_falling {
            client.play_sound(Sound::EntityPlayerAttackCrit, SoundCategory::Player, attacker.position.as_vec3(), 1.0, 1.0);
        }

        // client.write_packet(&EntityDamageS2c {
        //     entity_id: VarInt(**victim.eid),
        //     source_cause_id: VarInt(**attacker.eid),
        //     source_direct_id: VarInt(**attacker.eid),
        //     source_type_id: VarInt(registry.registries.get("minecraft:damage_type")),
        //     source_pos: None
        // });

        let apos = attacker.position.clone().as_vec3();

        if let Some(mut ae_client) = victim.client {
            ae_client.play_sound(
                Sound::EntityPlayerHurt,
                SoundCategory::Player,
                apos,
                1.0,
                1.0,
            );
            ae_client.set_velocity([dir.x * knockback_xz, knockback_y, dir.y * knockback_xz]);
        }
    }
}

pub fn remove_dead_entities(mut entity_health: Query<(Entity, &Health, &mut EntityStatuses), Changed<Health>>, mut commands: Commands, mut clients: Query<&mut Client, Changed<Health>>) {
    for (entity, hp, mut status) in entity_health.iter_mut() {
        if hp.0 <= 0.0 {
            if let Ok(mut client) = clients.get_mut(entity) {
                client.kill("you died");
                client.trigger_status(EntityStatus::AddDeathParticles);
                client.trigger_status(EntityStatus::PlayDeathSoundOrAddProjectileHitParticles);
            } else {
                commands.entity(entity).insert((Despawned,));
                status.trigger(EntityStatus::AddDeathParticles);
                status.trigger(EntityStatus::PlayDeathSoundOrAddProjectileHitParticles);
            }
        }
    }
}

#[derive(Event)]
pub struct SpawnPlayerEvent(pub Entity);

pub fn spawn_players(mut events: EventReader<SpawnPlayerEvent>, mut player_data: Query<(Entity, &mut RespawnPosition, &mut Health, &mut Saturation, &mut VisibleChunkLayer, &mut VisibleEntityLayers, &mut EntityLayerId, &Username, &mut Client, &mut OpLevel, &mut CommandScopes, &mut Position)>, layers: Query<Entity, (With<ChunkLayer>, With<EntityLayer>)>, mut commands: Commands, server: Res<Server>) {
    for event in events.read() {
        let layer = layers.single();

        if let Ok((entity, mut respawn_pos, mut health, mut saturation, mut visible_chunk_layer, mut visible_entity_layers, mut layer_id, username, mut client, mut op_level, mut permissions, mut pos)) = player_data.get_mut(event.0) {
            respawn_pos.pos = BlockPos::new(SPAWN_POS.x as i32, SPAWN_POS.y as i32, SPAWN_POS.z as i32);

            layer_id.0 = layer;
            visible_chunk_layer.0 = layer;
            visible_entity_layers.0.insert(layer);

            set_op_status(&mut client, username, &mut op_level, 4, &mut permissions);

            *health = Health(20.0);
            *saturation = Saturation(6.0);

            commands.entity(entity).insert((CombatState { last_attacked_tick: server.current_tick(), iframe_timer: 0, last_attacked_damage: 0.0 },));

            pos.set(SPAWN_POS);

            info!("{} spawned at {:?}", username.0, SPAWN_POS);
        }
    }
}

pub fn respawning(mut events: EventReader<RequestRespawnEvent>, mut event_writer: EventWriter<SpawnPlayerEvent>) {
    events.read().map(|v| event_writer.send(SpawnPlayerEvent(v.client))).count();
}
