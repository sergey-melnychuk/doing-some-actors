use std::collections::{HashSet, HashMap};
use crate::core::{Scheduler, AnyActor, Envelope, AnySender, start_actor_runtime};
use crate::pool::ThreadPool;
use std::time::{Instant, Duration};

struct Round {
    tag: String,
    size: usize,
}

impl Round {
    fn new(tag: &str, size: usize) -> Round {
        Round {
            tag: tag.to_string(),
            size,
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

enum Fan {
    Trigger { size: usize },
    Out { id: usize },
    In { id: usize },
}

struct Root {
    tag: String,
    size: usize,
    count: usize,
    epoch: usize,
    seen: HashSet<usize>,
}

impl Root {
    fn new(tag: &str) -> Root {
        Root {
            tag: tag.to_string(),
            size: 0,
            count: 0,
            epoch: 0,
            seen: HashSet::new(),
        }
    }
}

impl AnyActor for Root {
    fn receive(&mut self, envelope: Envelope, sender: &mut dyn AnySender) {
        if let Some(fan) = envelope.message.downcast_ref::<Fan>() {
            match fan {
                Fan::Trigger { size } => {
                    self.size = *size;
                    for id in 0..self.size {
                        let tag = format!("{}", id);
                        let env = Envelope { message: Box::new(Fan::Out { id }), from: self.tag.clone() };
                        sender.send(&tag, env)
                    }
                },
                Fan::In { id } => {
                    self.seen.insert(*id);
                    self.count += 1;
                    if self.count == self.size {
                        self.seen.clear();
                        self.count = 0;
                        println!("root completed the fanout of size: {} (epoch: {})", self.size, self.epoch);
                        let trigger = Box::new(Fan::Trigger { size: self.size });
                        let env = Envelope { message: trigger, from: self.tag.clone() };
                        sender.send(&self.tag, env);
                        self.epoch += 1;
                    }
                },
                _ => ()
            }
        }
    }
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
        } else if let Some(Fan::Out { id }) = envelope.message.downcast_ref::<Fan>() {
            let env = Envelope { message: Box::new(Fan::In { id: *id }), from: self.tag.clone() };
            sender.send(&envelope.from, env);
        } else {
            println!("unexpected message: {:?}", envelope.message.type_id());
        }
    }
}

struct Periodic {
    tag: String,
    at: Instant,
    timings: HashMap<usize, usize>,
    counter: usize,
}

impl Periodic {
    fn new(tag: &str) -> Periodic {
        Periodic {
            tag: tag.to_string(),
            at: Instant::now(),
            timings: HashMap::new(),
            counter: 0,
        }
    }
}

struct Tick {
    at: Instant,
}

impl AnyActor for Periodic {
    fn receive(&mut self, envelope: Envelope, sender: &mut dyn AnySender) {
        if let Some(Tick { at }) = envelope.message.downcast_ref::<Tick>() {
            self.at = Instant::now();
            let d = self.at.duration_since(*at).as_millis() as usize;
            if let Some(n) = self.timings.get_mut(&d) {
                *n += 1;
            } else {
                self.timings.insert(d, 1);
            }
            self.counter += 1;
            if self.counter % 1000 == 0 {
                let total: usize = self.timings.values().sum();
                let mut ds = self.timings.keys().into_iter().collect::<Vec<&usize>>();
                let mut sum: usize = 0;
                ds.sort();
                println!("timer latencies:");
                for d in ds {
                    let n = self.timings.get(d).unwrap();
                    sum += *n;
                    println!("\t{} ms\t: {}\t{}/{}", *d, *n, sum, total);
                }
                self.timings.clear();
            }
            let e = Envelope { message: Box::new(Tick { at: Instant::now() }), from: self.tag.to_string() };
            sender.delay(&self.tag, e, Duration::from_millis(10));
        }
    }
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

    scheduler.spawn("root", |tag| Box::new(Root::new(tag)));
    let trigger = Envelope { message: Box::new(Fan::Trigger { size: SIZE }), from: "root".to_string() };
    scheduler.send("root", trigger);

    scheduler.spawn("timer", |tag| Box::new(Periodic::new(tag)));
    let tick = Envelope { message: Box::new(Tick { at: Instant::now() }), from: "timer".to_string() };
    scheduler.send("timer", tick);

    let pool = ThreadPool::new(std::cmp::max(5, num_cpus::get()));
    start_actor_runtime(scheduler, pool, None);
}
