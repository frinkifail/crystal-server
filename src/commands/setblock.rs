use tracing::info;
use valence::{
    BlockPos, ChunkLayer,
    block::BlockKind,
    command::{handler::CommandResultEvent, parsers::Vec3},
    command_macros::Command,
    entity::Position,
    prelude::{EventReader, Query},
};

#[derive(Command)]
#[paths("setblock {location} {block}")]
pub struct SetBlockCommand {
    location: Vec3,
    block: String,
}

pub fn handle_setblock_command(
    mut events: EventReader<CommandResultEvent<SetBlockCommand>>,
    mut layers: Query<&mut ChunkLayer>,
    mut positions: Query<&mut Position>,
) {
    let mut layer = layers.single_mut();
    for event in events.read() {
        let kind = BlockKind::from_str(&event.result.block);
        if let Some(kind) = kind {
            let loc = event.result.location;
            let pos = positions.get_mut(event.executor).unwrap();
            let mut blockpos = BlockPos::new(0, 0, 0);
            blockpos.x = loc.x.get(pos.0.x as f32).round() as i32;
            blockpos.y = loc.y.get(pos.0.y as f32).round() as i32;
            blockpos.z = loc.z.get(pos.0.z as f32).round() as i32;
            // let pos = BlockPos::new(event.executor);
            layer.set_block(blockpos, kind.to_state());

            info!("set block at {} to {:#?}", blockpos, kind);
        }
    }
}
