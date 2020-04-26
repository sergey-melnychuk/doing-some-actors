use std::any::Any;
use std::collections::{HashMap, BinaryHeap};

use crate::pool::ThreadPool;
use std::sync::mpsc::channel;
use std::sync::{Mutex, Arc};
use std::time::{Instant, Duration};
use std::cmp::Ordering;
use std::ops::Add;

pub trait AnyActor {
    fn receive(&mut self, envelope: Envelope, sender: &mut dyn AnySender);
}

#[derive(Debug)]
pub struct Envelope {
    pub message: Box<dyn Any + Send>,
    pub from: String,
}

pub trait AnySender {
    fn send(&mut self, address: &str, message: Envelope);
    fn spawn(&mut self, address: &str, parent: &str, f: fn(&str, &str) -> Box<dyn AnyActor + Send>);
    fn delay(&mut self, address: &str, envelope: Envelope, duration: Duration);
}

impl AnySender for Memory<Envelope> {
    fn send(&mut self, address: &str, message: Envelope) {
        //println!("sending message from '{}' to '{}'", message.from, address);
        self.map.entry(address.to_string()).or_default().push(message);
    }

    fn spawn(&mut self, address: &str, parent: &str, f: fn(&str, &str) -> Box<dyn AnyActor + Send>) {
        println!("request for spawning actor '{}' of parent '{}'", address, parent);
        self.new.insert(address.to_string(), f(address, parent));
    }

    fn delay(&mut self, address: &str, envelope: Envelope, duration: Duration) {
        self.delay.push(Entry { at: Instant::now().add(duration), tag: address.to_string(), envelope });
    }
}

struct Memory<T: Any + Sized + Send> {
    map: HashMap<String, Vec<T>>,
    new: HashMap<String, Box<dyn AnyActor + Send>>,
    delay: Vec<Entry>,
}

impl<T: Any + Sized + Send> Memory<T> {
    fn new() -> Memory<T> {
        Memory {
            map: HashMap::new(),
            new: HashMap::new(),
            delay: Vec::new(),
        }
    }
}

#[derive(Default)]
pub struct Scheduler {
    pub actors: HashMap<String, Box<dyn AnyActor + Send>>,
    pub queue: HashMap<String, Vec<Envelope>>,
    pub tasks: BinaryHeap<Entry>,
}

impl Scheduler {
    pub fn spawn(&mut self, tag: &str, f: fn(&str) -> Box<dyn AnyActor + Send>) {
        let actor = f(tag);
        self.actors.insert(tag.to_string(), actor);
    }
    pub fn send(&mut self, tag: &str, envelope: Envelope) {
        self.queue.entry(tag.to_string()).or_default().push(envelope);
    }
}

enum Event {
    Mail { tag: String, actor: Box<dyn AnyActor + Send>, queue: Vec<Envelope> },
}

enum Action {
    Return { tag: String, actor: Box<dyn AnyActor + Send> },
    Spawn { tag: String, actor: Box<dyn AnyActor + Send> },
    Queue { tag: String, queue: Vec<Envelope> },
    Delay { entry: Entry },
}

pub struct Entry {
    at: Instant,
    tag: String,
    envelope: Envelope,
}

impl Eq for Entry {}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.at == other.at
    }
}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.at.cmp(&other.at)
    }
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.at.cmp(&other.at))
    }
}

// TODO return subscription/handle instead of blocking forever
pub fn start_actor_runtime(mut scheduler: Scheduler, mut pool: ThreadPool) {
    const THROUGHPUT: usize = 1;

    let (events_tx, events_rx) = channel();
    let events_rx = Arc::new(Mutex::new(events_rx));
    let (actions_tx, actions_rx) = channel();

    for (tag, queue) in scheduler.queue.drain() {
        actions_tx.send(Action::Queue { tag, queue }).unwrap();
    }

    for _ in 1..pool.size() {
        let rx = Arc::clone(&events_rx);
        let tx = actions_tx.clone();

        pool.submit(move || {
            let mut memory: Memory<Envelope> = Memory::new();
            loop {
                let event = rx.lock().unwrap().try_recv();
                if let Ok(x) = event {
                    match x {
                        Event::Mail { tag, mut actor, mut queue } => {
                            //println!("[worker thread] event.mail for tag '{}': {} messages", id, tag, queue.len());
                            if queue.len() > THROUGHPUT {
                                let remaining = queue.split_off(THROUGHPUT);
                                tx.send(Action::Queue { tag: tag.clone(), queue: remaining }).unwrap();
                            }
                            for envelope in queue.into_iter() {
                                actor.receive(envelope, &mut memory);
                            }
                            tx.send(Action::Return { tag, actor }).unwrap();
                        }
                    }
                    for (tag, actor) in memory.new.drain().into_iter() {
                        let action = Action::Spawn { tag, actor };
                        tx.send(action).unwrap();
                    }
                    for (tag, queue) in memory.map.drain().into_iter() {
                        let action = Action::Queue { tag, queue };
                        tx.send(action).unwrap();
                    }
                    for entry in memory.delay.drain(..).into_iter() {
                        let action = Action::Delay { entry };
                        tx.send(action).unwrap();
                    }
                }
            }
        });
    }

    pool.submit(move || {
        let mut tick: usize = 0;
        let mut messages: usize = 0;
        let mut start = Instant::now();
        loop {
            let action = actions_rx.try_recv();
            if let Ok(x) = action {
                match x {
                    Action::Return { tag, actor } => {
                        //println!("return of '{}'", tag);
                        let mut queue = scheduler.queue.remove(&tag).unwrap_or_default();
                        if !queue.is_empty() {
                            if queue.len() > THROUGHPUT {
                                let remaining = queue.split_off(THROUGHPUT);
                                scheduler.queue.insert(tag.clone(), remaining);
                            }
                            actions_tx.send(Action::Queue { tag: tag.clone(), queue }).unwrap();
                        }
                        scheduler.actors.insert(tag, actor);
                    },
                    Action::Queue { tag, queue } => {
                        messages += queue.len();
                        match scheduler.actors.remove(&tag) {
                            Some(actor) => {
                                let event = Event::Mail { tag, actor, queue };
                                events_tx.send(event).unwrap();
                            },
                            None => {
                                let q = scheduler.queue.entry(tag.clone()).or_default();
                                for e in queue {
                                    q.push(e);
                                }
                            }
                        }
                    },
                    Action::Spawn { tag, actor } => {
                        scheduler.actors.insert(tag, actor);
                    },
                    Action::Delay { entry } => {
                        scheduler.tasks.push(entry);
                    }
                }
            }

            // TODO Collect statistic about running time of this loop to estimate `precision`
            let precision = Duration::from_millis(1);
            let now = Instant::now().add(precision);
            while scheduler.tasks.peek().map(|e| e.at <= now).unwrap_or_default() {
                if let Some(Entry { at: _, tag, envelope }) = scheduler.tasks.pop() {
                    let action = Action::Queue { tag, queue: vec![envelope] };
                    actions_tx.send(action).unwrap();
                }
            }

            tick += 1;
            if tick % 1000000 == 0 {
                if messages > 0 {
                    let elapsed = start.elapsed().as_millis() as usize;
                    let rate = (messages as f64 / (elapsed as f64 / 1000.0)) as usize;
                    println!("tick={}M messages={} elapsed: {} ms | rate = {} mps",
                             tick / 1000000, messages, elapsed, rate);
                }
                start = Instant::now();
                messages = 0;
            }
        }
    });
}
