use crate::core::{Envelope, AnySender, AnyActor, Scheduler, start_actor_runtime};
use crate::pool::ThreadPool;

#[allow(dead_code)]
struct PingPong {
    tag: String,
    count: usize,
}

impl PingPong {
    #[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
#[derive(Default)]
struct Counter {
    count: usize,
}

#[allow(dead_code)]
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

#[derive(Default)]
struct Root {
    epochs: usize,
}

impl Root {
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

#[allow(dead_code)]
fn high_level_example() {
    let mut scheduler = Scheduler::default();

    scheduler.spawn("root", |_| Box::new(Root::default()));
    scheduler.spawn("ping", |tag| Box::new(PingPong::new(tag)));
    scheduler.spawn("pong", |tag| Box::new(PingPong::new(tag)));

    let tick = Envelope { message: Box::new(()), from: "root".to_string() };
    scheduler.send("root", tick);
    let ping = Envelope { message: Box::new("ping".to_string()), from: "pong".to_string() };
    scheduler.send("ping", ping);

    let pool = ThreadPool::new(num_cpus::get());
    start_actor_runtime(scheduler, pool);
}
