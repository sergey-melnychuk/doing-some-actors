use std::sync::mpsc::{channel, Sender};

extern crate doing_some_actors;
use doing_some_actors::api::{AnyActor, Envelope, AnySender};
use doing_some_actors::core::System;

struct Message(usize, Sender<usize>);

#[derive(Default)]
struct State;

impl AnyActor for State {
    fn receive(&mut self, envelope: Envelope, _sender: &mut dyn AnySender) {
        if let Some(msg) = envelope.message.downcast_ref::<Message>() {
            println!("Actor 'State' received a message: {}", msg.0);
            msg.1.send(msg.0).unwrap();
        }
    }
}

fn main() {
    let sys = System::default();
    let run = sys.run();

    run.spawn_default::<State>("x");

    let (tx, rx) = channel();
    run.send("x", Envelope::of(Message(42, tx), ""));

    rx.recv().unwrap();
    run.shutdown();
}
