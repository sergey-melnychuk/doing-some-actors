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

#[derive(Clone)]
struct Acc {
    name: String,
    hits: usize,
}

impl AnyActor for Round {
    fn receive(&mut self, envelope: Envelope, sender: &mut dyn AnySender) {
        if let Some(hit) = envelope.message.downcast_ref::<Hit>() {
            if hit.0 == 0 {
                println!("hit went around");
            }
            let next = if hit.0 + 1 == self.size as usize { 0 } else { hit.0 + 1 };
            let tag = format!("{}", next);
            //println!("tag:'{}' hit:{} next:'{}'", self.tag, hit.0, tag);
            let m = Hit(next);
            let envelope = Envelope { message: Box::new(m), from: self.tag.clone() };
            sender.send(&tag, envelope);
        } else if let Some(acc) = envelope.message.downcast_ref::<Acc>() {
            if acc.name == self.tag {
                println!("acc '{}' went around", acc.name);
            }
            let next = if acc.hits + 1 == self.size as usize { 0 } else { acc.hits + 1 };
            let tag = format!("{}", next);
            //println!("tag:'{}' acc: name='{}' hits={}", self.tag, acc.name, acc.hits);
            let m = Acc { name: acc.name.clone(), hits: next };
            let env = Envelope { message: Box::new(m), from: self.tag.clone() };
            sender.send(&tag, env)
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
    const SIZE: u32 = 1000000;

    let mut scheduler = Scheduler::default();
    scheduler.send("0", Envelope { message: Box::new(Hit(0)), from: String::default() });

    for id in 0..SIZE {
        let tag = format!("{}", id);
        scheduler.spawn(&tag, |tag| Box::new(Round::new(tag, SIZE)));
    }

    for id in 0..SIZE {
        let tag = format!("{}", id);
        let env = Envelope { message: Box::new(Acc { name: tag.clone(), hits: 0 }), from: tag.clone() };
        scheduler.send(&tag, env);
    }

    let mut pool = ThreadPool::new(num_cpus::get());
    start_actor_runtime(scheduler, pool);
}
