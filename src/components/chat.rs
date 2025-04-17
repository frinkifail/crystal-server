use valence::{client::Client, message::ChatMessageEvent, prelude::EventReader, prelude::*};

pub fn chat_message_event(mut events: EventReader<ChatMessageEvent>, mut clients: Query<(&mut Client, &Username)>) {
    for event in events.read() {
        let username = clients.get(event.client).unwrap().1.clone();
        let message = event.message.clone();
        let username_text = ("<".to_owned() + &username.0 + "> ").color(Color::AQUA);

        for (mut client, _) in clients.iter_mut() {
            client.send_chat_message(username_text.clone() + String::from(message.clone()).color(Color::WHITE));
        }
    }
}