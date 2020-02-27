use std::any::Any;
use std::collections::HashMap;
use std::fmt::Debug;

fn main() {
    let mut sender = Memory::new();

    let mut counter = Counter::default();
    counter.receive(Query::Hit, &mut sender);
    counter.receive(Query::Get, &mut sender);

    let mut system = System::new();
    system.add("ping".to_string(), Box::new(Counter::default()));

    system.get("ping").unwrap().receive(Query::Hit, &mut sender);
    system.get("ping").unwrap().receive(Query::Get, &mut sender);

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
    fn send(&mut self, address: &String, message: T);
}

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
    fn send(&mut self, address: &String, message: T) {
        println!("Sending '{:?}' to '{}'", message, address);
        self.map.entry(address.clone()).or_default().push(message);
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
