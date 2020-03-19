use std::collections::HashSet;
use crate::untyped::{Scheduler, AnyActor, Envelope, AnySender, start_actor_runtime};
use crate::pool::ThreadPool;

struct Round {
    tag: String,
    size: usize,
    hits: usize,
}

impl Round {
    fn new(tag: &str, size: usize) -> Round {
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
    zero: usize,
    hits: usize,
}

impl AnyActor for Round {
    fn receive(&mut self, envelope: Envelope, sender: &mut dyn AnySender) {
        if let Some(hit) = envelope.message.downcast_ref::<Hit>() {
            if hit.0 > 0 && hit.0 % self.size == 0 {
                println!("the hit went around: hits={}", hit.0);
            }
            let next = (hit.0 + 1) % self.size;
            let tag = format!("{}", next);
            //println!("tag:{} hits={} next={}", self.tag, hit.0, tag);
            let m = Hit(hit.0 + 1);
            let envelope = Envelope { message: Box::new(m), from: self.tag.clone() };
            sender.send(&tag, envelope);
        } else if let Some(acc) = envelope.message.downcast_ref::<Acc>() {
            if acc.name == self.tag && acc.hits > 0 {
                println!("acc '{}' went around: hits={}", acc.name, acc.hits);
            }
            let next = (acc.zero + acc.hits + 1) % self.size;
            let tag = format!("{}", next);
            //println!("tag:{} [acc] name={} hits={} next={}", self.tag, acc.name, acc.hits, next);
            let m = Acc { name: acc.name.clone(), zero: acc.zero, hits: acc.hits + 1 };
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
    const SIZE: usize = 100000;

    let mut scheduler = Scheduler::default();

    for id in 0..SIZE {
        let tag = format!("{}", id);
        scheduler.spawn(&tag, |tag| Box::new(Round::new(tag, SIZE)));
    }

    scheduler.send("0", Envelope { message: Box::new(Hit(0)), from: String::default() });
    for id in 0..1000 {
        let tag = format!("{}", id);
        let acc = Acc { name: tag.clone(), zero: id, hits: 0 };
        let env = Envelope { message: Box::new(acc), from: tag.clone() };
        scheduler.send(&tag, env);
    }

    let mut pool = ThreadPool::new(num_cpus::get());
    start_actor_runtime(scheduler, pool);
}
