extern crate rand;

use rand::Rng;
use std::collections::HashSet;
use crate::untyped::{Scheduler, AnyActor, Envelope, AnySender, start_actor_runtime};
use crate::pool::ThreadPool;

struct Round {
    tag: String,
    size: u32,
    hits: usize,
}

impl Round {
    fn new(tag: &str, size: u32) -> Round {
        Round {
            tag: tag.to_string(),
            size,
            hits: 0,
        }
    }
}

struct Hit(usize);

impl AnyActor for Round {
    fn receive(&mut self, envelope: Envelope, sender: &mut dyn AnySender) {
        if let Some(hit) = envelope.message.downcast_ref::<Hit>() {
            let mut rng = rand::thread_rng();
            let next = rng.gen_range(0, self.size);
            let tag = format!("{}", next);
            println!("tag:'{}' hit:{} next:'{}'", self.tag, hit.0, tag);

            let envelope = Envelope { message: Box::new(Hit(hit.0 + 1)), from: self.tag.clone() };
            sender.send(&tag, envelope);
        } else {
            println!("unexpected message: {:?}", envelope.message.type_id());
        }
    }
}

fn peers(size: u32) -> HashSet<String> {
    let mut peers = HashSet::with_capacity(size as usize);
    for id in 0..size {
        let tag = format!("{}", id);
        peers.insert(tag);
    }
    peers
}

pub fn run() {
    let mut scheduler = Scheduler::default();

    const SIZE: u32 = 10;
    for id in 0..SIZE {
        let tag = format!("{}", id);
        scheduler.spawn(&tag, |tag| Box::new(Round::new(tag, SIZE)));
    }

    scheduler.send("0", Envelope { message: Box::new(Hit(0)), from: String::default() });

    let mut pool = ThreadPool::new(num_cpus::get());
    start_actor_runtime(pool, scheduler);
}
