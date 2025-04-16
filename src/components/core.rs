use valence::{command::scopes::CommandScopes, op_level::OpLevel, prelude::*};

#[derive(Resource)]
#[allow(dead_code)]
pub struct ServerVersion(pub String);

pub fn set_op_status(client: &mut Client, username: &Username, which: &mut OpLevel, state: Option<bool>, permissions: &mut CommandScopes) {
    let level = if let Some(state) = state { if state { 4 } else { 0 } } else { if which.get() == 4 { 0 } else { 4 } };
    which.set(level);
    if level == 4 { permissions.add("crystal.admin"); } else { permissions.remove("crystal.admin"); }
    println!("{} {}", if level == 4 { "[op] added server operator status for" } else { "[op] revoked operator status for" }, username.0);
    if level == 4 { client.send_chat_message(new_crystal_message(format!("Made {} a server operator", username.0).into())); }
}

pub fn new_crystal_message(message: Text) -> Text {
    "[Crystal] ".color(Color::RED) + "".color(Color::GOLD) + message
}
