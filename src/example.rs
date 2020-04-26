use crate::core::{Envelope, AnySender, AnyActor, Scheduler, start_actor_runtime};
use std::collections::HashMap;
use crate::pool::ThreadPool;

struct PingPong {
    tag: String,
    count: usize,
}

impl PingPong {
    fn new(tag: &str) -> PingPong {
        PingPong {
            tag: tag.to_string(),
            count: 0,
        }
    }
}

impl AnyActor for PingPong {
    fn receive(&mut self, envelope: Envelope, sender: &mut dyn AnySender) {
        if let Some(s) = envelope.message.downcast_ref::<String>() {
            println!("Actor '{}' (count={}) received message '{}'", self.tag, self.count, s);
            if self.count >= 3 {
                return;
            }
            self.count += 1;
            if s == "ping" {
                let r = Envelope { message: Box::new("pong".to_string()), from: self.tag.clone() };
                sender.send(&envelope.from, r);
            } else if s == "pong" {
                let r = Envelope { message: Box::new("ping".to_string()), from: self.tag.clone() };
                sender.send(&envelope.from, r);
            } else {
                println!("PingPong actor received unexpected string: '{}'", s);
                sender.spawn("summator", &self.tag,
                             |tag, parent| Box::new(Child::new(tag, parent)));
                let m = Envelope { message: Box::new(42 as u64), from: self.tag.clone() };
                sender.send("summator", m);
            }
        }
    }
}

struct Child {
    up: String,
    tag: String,
    sum: u64,
}

impl Child {
    fn new(tag: &str, up: &str) -> Child {
        Child {
            tag: tag.to_string(),
            up: up.to_string(),
            sum: 0,
        }
    }
}

impl AnyActor for Child {
    fn receive(&mut self, envelope: Envelope, sender: &mut dyn AnySender) {
        println!("Actor '{}' receive2 running: {:?}", self.tag, envelope);
        if let Some(x) = envelope.message.downcast_ref::<u64>() {
            println!("Actor '{}' received message '{}'", self.tag, x);
            self.sum += *x;
        } else if let Some(x) = envelope.message.downcast_ref::<String>() {
            println!("Actor '{}' received message '{}'", self.tag, x);
            if x == "get" {
                let r = Envelope { message: Box::new(self.sum), from: self.tag.clone() };
                sender.send(&envelope.from, r);
            }
        }
    }
}

#[derive(Default)]
struct Counter {
    count: usize,
}

#[derive(Debug)]
enum Something {
    Thing1(u64),
    Thing2(String, bool),
    RawThing(Vec<u8>),
}

impl AnyActor for Counter {
    fn receive(&mut self, envelope: Envelope, sender: &mut dyn AnySender) {
        if let Some(s) = envelope.message.downcast_ref::<usize>() {
            println!("Counter received: {}", s);
            let r = Envelope { message: Box::new("sup?".to_string()), from: "anonymous".to_string() };
            sender.send("ping", r);
        } else if let Some(smth) = envelope.message.downcast_ref::<Something>() {
            println!("counter matched something: {:?}", smth);
            match smth {
                Something::Thing1(n) => println!("thing1! n={}", n),
                Something::Thing2(s, f) => println!("thing2! s={}, f={}", s, f),
                Something::RawThing(bytes) => println!("raw thing! bytes={:?}", bytes)
            }
        }
    }
}

struct Root {
    epochs: usize,
}

impl Root {
    fn new() -> Root {
        Root {
            epochs: 0,
        }
    }

    fn tick(&mut self) -> bool {
        self.epochs += 1;
        self.epochs < 10
    }
}

impl AnyActor for Root {
    fn receive(&mut self, envelope: Envelope, sender: &mut dyn AnySender) {
        if self.tick() {
            // effectively this is an infinite loop
            sender.send("root", envelope);
        }
    }
}

fn low_level_example() {
    let mut scheduler = Scheduler::default();

    scheduler.spawn("ping", |tag| Box::new(PingPong::new(tag)));
    scheduler.spawn("pong", |tag| Box::new(PingPong::new(tag)));

    let e = Envelope { message: Box::new("ping".to_string()), from: "pong".to_string() };
    scheduler.queue.insert("ping".to_string(), vec![e]);
    let e = Envelope { message: Box::new("pong".to_string()), from: "ping".to_string() };
    scheduler.queue.insert("pong".to_string(), vec![e]);
    let e = Envelope { message: Box::new("sup?".to_string()), from: "none".to_string() };
    scheduler.queue.insert("ping".to_string(), vec![e]);

    scheduler.spawn("counter", |_| Box::new(Counter::default()));
    let e = Envelope { message: Box::new(42 as usize), from: "unknown".to_string() };
    scheduler.queue.insert("counter".to_string(), vec![e]);

    let mut epoch: usize = 0;
    while !scheduler.queue.is_empty() {
        println!("epoch: {}", epoch);
        epoch += 1;

        let mut mem: Memory<Envelope> = Memory::new();

        let q: HashMap<String, Vec<Envelope>> = scheduler.queue.drain().collect();
        println!("queue: {:?}", q.keys());
        println!("actors: {:?}", scheduler.actors.keys());
        for (addr, env) in q.into_iter() {
            let mut actor = scheduler.actors.get_mut(&addr).unwrap();
            for e in env {
                actor.receive(e, &mut mem);
            }

            for (tag, actor) in mem.new.drain() {
                if scheduler.actors.contains_key(&tag) {
                    println!("TODO spawning actor at taken address '{}' was requested", tag);
                } else {
                    println!("spawning actor '{}'", tag.clone());
                    scheduler.actors.insert(tag, actor);
                }
            }
        }

        for (addr, vec) in mem.map.into_iter() {
            scheduler.queue.insert(addr, vec);
        }
        println!("---");
    }

    let mut m: Memory<Envelope> = Memory::new();
    let e = Envelope { message: Box::new(42 as u64), from: "none".to_string() };
    scheduler.actors.get_mut("summator").unwrap().receive(e, &mut m);

    let e = Envelope { message: Box::new(Something::Thing1(91)), from: "none".to_string() };
    scheduler.actors.get_mut("counter").unwrap().receive(e, &mut m);

    let e = Envelope { message: Box::new(Something::Thing2("enabled".to_string(), false)), from: "none".to_string() };
    scheduler.actors.get_mut("counter").unwrap().receive(e, &mut m);

    let e = Envelope { message: Box::new(Something::RawThing(vec![42, 43, 44])), from: "none".to_string() };
    scheduler.actors.get_mut("counter").unwrap().receive(e, &mut m);
}

fn high_level_example() {
    let mut scheduler = Scheduler::default();

    scheduler.spawn("root", |_| Box::new(Root::new()));
    scheduler.spawn("ping", |tag| Box::new(PingPong::new(tag)));
    scheduler.spawn("pong", |tag| Box::new(PingPong::new(tag)));

    let tick = Envelope { message: Box::new(()), from: "root".to_string() };
    scheduler.send("root", tick);
    let ping = Envelope { message: Box::new("ping".to_string()), from: "pong".to_string() };
    scheduler.send("ping", ping);

    let pool = ThreadPool::new(num_cpus::get());
    start_actor_runtime(scheduler, pool);
}
