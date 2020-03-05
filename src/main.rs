use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt::Debug;

fn main() {
    /*
    let mut sender = Memory::new();

    let mut counter = Counter::default();
    counter.receive(Query::Hit, &mut sender);
    counter.receive(Query::Get, &mut sender);

    let mut system = System::new();
    system.add("ping".to_string(), Box::new(Counter::default()));

    system.get("ping").unwrap().receive(Query::Hit, &mut sender);
    system.get("ping").unwrap().receive(Query::Get, &mut sender);

    println!("sender: {:?}", sender);

    if let Some(ms) = sender.map.get("addr").take() {
        for m in ms {
            println!("{:?}", m);
        }
    }

    let mut output = Output;
    let mut untyped = Untyped::default();
    output.receive("hello there".to_string(), &mut untyped);
    output.receive(42, &mut untyped);
    output.receive(HashMap::<(), ()>::new(), &mut untyped);
    output.receive(Query::Hit, &mut untyped);
    output.receive(Query::Get, &mut untyped);
    output.receive(Response::Count(42), &mut untyped);

    println!("{:?}", untyped);
    let opt = untyped.data.remove("addr");
    if let Some(ms) = opt {
        for m in ms {
            output.receive(m, &mut untyped);
        }
    }
    */

    let mut scheduler = Scheduler::default();

    scheduler.spawn("ping".to_string(), |tag| Box::new(PingPong::new(tag)));
    scheduler.spawn("pong".to_string(), |tag| Box::new(PingPong::new(tag)));

    {
        let env = Envelope {
            message: Box::new("ping".to_string()),
            from: "pong".to_string(),
        };
        scheduler.queue.insert("ping".to_string(), vec![env]);
    }
    {
        let env = Envelope {
            message: Box::new("pong".to_string()),
            from: "ping".to_string(),
        };
        scheduler.queue.insert("pong".to_string(), vec![env]);
    }
    {
        let env = Envelope {
            message: Box::new("sup?".to_string()),
            from: "none".to_string(),
        };
        scheduler.queue.insert("ping".to_string(), vec![env]);
    }

    let mut epoch: usize = 0;
    while !scheduler.queue.is_empty() {
        println!("\nepoch: {}", epoch);
        epoch += 1;

        let mut mem: Memory<Envelope> = Memory::new();

        let q: HashMap<String, Vec<Envelope>> = scheduler.queue.drain().collect();
        println!("queue: {:?}", q.keys());
        println!("actors: {:?}", scheduler.actors.keys());
        for (addr, env) in q.into_iter() {
            let mut actor = scheduler.actors.get_mut(&addr).unwrap();
            for e in env {
                actor.receive2(e, &mut mem);
            }

            for (tag, actor) in mem.new.drain() {
                println!("spawning actor '{}'", tag.clone());
                scheduler.actors.insert(tag, actor);
            }
        }

        for (addr, vec) in mem.map.into_iter() {
            scheduler.queue.insert(addr, vec);
        }
    }

    let e = Envelope {
        message: Box::new(42 as u64),
        from: "none".to_string(),
    };
    let mut m: Memory<Envelope> = Memory::new();
    scheduler.actors.get_mut("summator").unwrap().receive2(e, &mut m);
}

#[derive(Debug)]
struct Envelope {
    message: Box<dyn Any>,
    from: String,
}

#[derive(Default)]
struct Scheduler {
    actors: HashMap<String, Box<dyn AnyActor>>,
    queue: HashMap<String, Vec<Envelope>>,
}

impl Scheduler {
    fn spawn(&mut self, tag: String, f: fn(String) -> Box<dyn AnyActor>) {
        let actor = f(tag.clone());
        self.actors.insert(tag, actor);
    }
}

trait AnyActor {
    fn receive2(&mut self, envelope: Envelope, sender: &mut dyn AnySender);
}

impl AnyActor for Counter {
    fn receive2(&mut self, envelope: Envelope, sender: &mut dyn AnySender) {
        if is_string(envelope.message.as_ref()) {
            if let Some(s) = envelope.message.downcast_ref::<String>() {
                println!("AnyActor receive2: {}", s);
                let response = Envelope {
                    message: Box::new("sup?".to_string()),
                    from: "anonymous".to_string(),
                };
                sender.send2("ping", response);
            }
        }
    }
}

struct Output;

impl<Q: Any + Sized + Debug> Actor<Q, String> for Output {
    fn receive(&mut self, message: Q, sender: &mut dyn Sender<String>) {
        println!("Received '{:?}' by Output", message);

        let msg = &message as &dyn Any;
        if is_string(msg) {
            if let Some(s) = msg.downcast_ref::<String>() {
                println!("Matched String from Any: '{}'", s);
                let response = "sup?".to_string();
                sender.send("addr", response);
            }
        }
    }
}

fn is_string(s: &dyn Any) -> bool {
    TypeId::of::<String>() == s.type_id()
}

struct System<Q: Any + Sized, R: Any + Sized> {
    actors: HashMap<String, Box<dyn Actor<Q, R>>>,
}

impl<Q: Any + Sized, R: Any + Sized> System<Q, R> {
    fn new() -> System<Q, R> {
        System {
            actors: HashMap::new(),
        }
    }

    fn add(&mut self, address: String, actor: Box<dyn Actor<Q, R>>) {
        self.actors.insert(address, actor);
    }

    fn get(&mut self, address: &str) -> Option<&mut Box<dyn Actor<Q, R>>> {
        self.actors.get_mut(address)
    }
}

trait Sender<T: Any> {
    fn send(&mut self, address: &str, message: T);
}

trait AnySender {
    fn send2(&mut self, address: &str, message: Envelope);
    fn spawn2(&mut self, address: &str, parent: &str, f: fn(String, String) -> Box<dyn AnyActor>);
}

impl AnySender for Memory<Envelope> {
    fn send2(&mut self, address: &str, message: Envelope) {
        println!("sending any '{:?}' to address '{}'", message, address);
        self.map.entry(address.to_string()).or_default().push(message);
    }

    fn spawn2(&mut self, address: &str, parent: &str, f: fn(String, String) -> Box<dyn AnyActor>) {
        self.new.insert(address.to_string(), f(address.to_string(), parent.to_string()));
    }
}

struct PingPong {
    tag: String,
    count: usize,
}

impl PingPong {
    fn new(tag: String) -> PingPong {
        PingPong {
            tag,
            count: 0,
        }
    }
}

impl AnyActor for PingPong {
    fn receive2(&mut self, envelope: Envelope, sender: &mut dyn AnySender) {
        if is_string(envelope.message.as_ref()) {
            if let Some(s) = envelope.message.downcast_ref::<String>() {
                println!("Actor '{}' (count={}) received message '{}'", self.tag, self.count, s);
                if self.count >= 3 {
                    return;
                }
                self.count += 1;
                if s == "ping" {
                    let r = Envelope {
                        message: Box::new("pong".to_string()),
                        from: self.tag.clone(),
                    };
                    sender.send2(&envelope.from, r);
                } else if s == "pong" {
                    let r = Envelope {
                        message: Box::new("ping".to_string()),
                        from: self.tag.clone(),
                    };
                    sender.send2(&envelope.from, r);
                } else {
                    println!("PingPong actor received unexpected string: {}", s);
                    sender.spawn2("summator", &self.tag, |tag, parent| Box::new(Child::new(tag, parent)));
                    let m = Envelope {
                        message: Box::new(42 as u64),
                        from: self.tag.clone(),
                    };
                    sender.send2("summator", m);
                }
            }
        }
    }
}

struct Child {
    tag: String,
    up: String,
    sum: u64,
}

impl Child {
    fn new(tag: String, up: String) -> Child {
        Child {
            tag,
            up,
            sum: 0,
        }
    }
}

impl AnyActor for Child {
    fn receive2(&mut self, envelope: Envelope, sender: &mut dyn AnySender) {
        println!("Actor '{}' receive2 running: {:?}", self.tag, envelope);
        if TypeId::of::<u64>() == envelope.message.as_ref().type_id() {
            if let Some(x) = envelope.message.downcast_ref::<u64>() {
                println!("Actor '{}' received message '{}'", self.tag, x);
                self.sum += *x;
            }
        } else if TypeId::of::<String>() == envelope.message.as_ref().type_id() {
            if TypeId::of::<String>() == envelope.message.type_id() {
                if let Some(x) = envelope.message.downcast_ref::<String>() {
                    println!("Actor '{}' received message '{}'", self.tag, x);
                    if x == "get" {
                        let r = Envelope {
                            message: Box::new(self.sum),
                            from: self.tag.clone(),
                        };
                        sender.send2(&envelope.from, r);
                    }
                }
            }
        }
    }
}

struct Memory<T: Any + Sized> {
    map: HashMap<String, Vec<T>>,
    new: HashMap<String, Box<dyn AnyActor>>,
}

impl<T: Any + Sized> Memory<T> {
    fn new() -> Memory<T> {
        Memory {
            map: HashMap::new(),
            new: HashMap::new(),
        }
    }
}

impl<T: Any + Sized + Debug> Sender<T> for Memory<T> {
    fn send(&mut self, address: &str, message: T) {
        println!("Sending '{:?}' to '{}'", message, address);
        self.map.entry(address.to_string()).or_default().push(message);
    }
}

#[derive(Default, Debug)]
struct Untyped<T: Any + Sized> {
    data: HashMap<String, Vec<T>>,
}

impl<T: Any + Sized + Debug> Sender<T> for Untyped<T> {
    fn send(&mut self, address: &str, message: T) {
        println!("Sent '{:?}' to '{}' via Untyped", message, address);
        self.data.entry(address.to_string()).or_default().push(message);
    }
}

trait Actor<Q: Any + Sized, R: Any + Sized> {
    fn receive(&mut self, message: Q, sender: &mut dyn Sender<R>);
}

// trait Message: Any + Sized {}
// impl<T> Message for T where T: Any + Sized {}

#[derive(Default)]
struct Counter {
    count: usize,
}

#[derive(Debug)]
enum Query {
    Hit,
    Get,
}

#[derive(Debug)]
enum Response {
    Count(usize),
}

impl Actor<Query, Response> for Counter {
    fn receive(&mut self, message: Query, sender: &mut dyn Sender<Response>) {
        println!("Received '{:?}' by actor", message);
        match message {
            Query::Hit => self.count += 1,
            Query::Get => {
                let response = Response::Count(self.count);
                sender.send(&"addr".to_string(), response);
            }
        }
    }
}
