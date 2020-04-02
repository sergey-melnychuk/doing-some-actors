use std::any::Any;
use std::fmt::Debug;
use std::collections::HashMap;

pub fn run() {
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
}

struct Output;

impl<Q: Any + Sized + Debug> Actor<Q, String> for Output {
    fn receive(&mut self, message: Q, sender: &mut dyn Sender<String>) {
        println!("Received by Output: message of type '{}'", std::any::type_name::<Q>());
        if let Some(s) = (&message as &dyn Any).downcast_ref::<String>() {
            println!("Matched String from Any: '{}'", s);
            sender.send("addr", "sup?".to_string());
        }
    }
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

#[derive(Debug)]
struct Memory<T: Any + Sized> {
    map: HashMap<String, Vec<T>>,
}

impl<T: Any + Sized> Memory<T> {
    fn new() -> Memory<T> {
        Memory {
            map: HashMap::new(),
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

#[derive(Debug)]
enum Query {
    Hit,
    Get,
}

#[derive(Debug)]
enum Response {
    Count(usize),
}

#[derive(Default)]
struct Counter {
    count: usize,
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
