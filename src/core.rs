use std::any::Any;
use std::collections::{HashMap, BinaryHeap, HashSet};

use crate::pool::ThreadPool;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, Arc};
use std::time::{Instant, Duration, SystemTime};
use std::cmp::Ordering;
use std::ops::Add;
use std::fmt;

pub trait AnyActor {
    fn receive(&mut self, envelope: Envelope, sender: &mut dyn AnySender);
}

#[derive(Debug)]
pub struct Envelope {
    pub message: Box<dyn Any + Send>,
    pub from: String,
}

impl Default for Envelope {
    fn default() -> Self {
        Envelope {
            message: Box::new(()),
            from: String::default(),
        }
    }
}

pub trait AnySender {
    fn send(&mut self, address: &str, message: Envelope);
    fn spawn(&mut self, address: &str, f: fn(&str) -> Box<dyn AnyActor + Send>);
    fn delay(&mut self, address: &str, envelope: Envelope, duration: Duration);
    fn stop(&mut self, address: &str);
}

impl AnySender for Memory<Envelope> {
    fn send(&mut self, address: &str, message: Envelope) {
        self.map.entry(address.to_string()).or_default().push(message);
    }

    fn spawn(&mut self, address: &str, f: fn(&str) -> Box<dyn AnyActor + Send>) {
        self.new.insert(address.to_string(), f(address));
    }

    fn delay(&mut self, address: &str, envelope: Envelope, duration: Duration) {
        let at = Instant::now().add(duration);
        let entry = Entry { at, tag: address.to_string(), envelope };
        self.delay.push(entry);
    }

    fn stop(&mut self, address: &str) {
        self.stop.insert(address.to_string());
    }
}

impl Memory<Envelope> {
    fn drain(&mut self, tx: &Sender<Action>) {
        for (tag, actor) in self.new.drain().into_iter() {
            let action = Action::Spawn { tag, actor };
            tx.send(action).unwrap();
        }
        for (tag, queue) in self.map.drain().into_iter() {
            let action = Action::Queue { tag, queue };
            tx.send(action).unwrap();
        }
        for entry in self.delay.drain(..).into_iter() {
            let action = Action::Delay { entry };
            tx.send(action).unwrap();
        }
        for tag in self.stop.drain().into_iter() {
            let action = Action::Stop { tag };
            tx.send(action).unwrap();
        }
    }
}

#[derive(Default)]
struct Memory<T: Any + Sized + Send> {
    map: HashMap<String, Vec<T>>,
    new: HashMap<String, Box<dyn AnyActor + Send>>,
    delay: Vec<Entry>,
    stop: HashSet<String>,
}

#[derive(Default)]
pub struct Scheduler {
    actors: HashMap<String, Box<dyn AnyActor + Send>>,
    queue: HashMap<String, Vec<Envelope>>,
    tasks: BinaryHeap<Entry>,
    active: HashSet<String>,
}

impl Scheduler {
    pub fn send(&mut self, tag: &str, envelope: Envelope) {
        self.queue.entry(tag.to_string()).or_default().push(envelope);
    }

    pub fn spawn(&mut self, tag: &str, f: fn(&str) -> Box<dyn AnyActor + Send>) {
        if !self.active.contains(tag) {
            let actor = f(tag);
            self.actors.insert(tag.to_string(), actor);
            self.active.insert(tag.to_string());
        }
    }

    pub fn delay(&mut self, address: &str, envelope: Envelope, duration: Duration) {
        let at = Instant::now().add(duration);
        let entry = Entry { at, tag: address.to_string(), envelope };
        self.tasks.push(entry);
    }

    fn stop(&mut self, address: &str) {
        self.active.remove(address);
        self.actors.remove(address);
        self.queue.remove(address);
    }
}

// received by Worker threads
enum Event {
    Mail { tag: String, actor: Box<dyn AnyActor + Send>, queue: Vec<Envelope> },
}

// received by the Scheduler thread
enum Action {
    Return { tag: String, actor: Box<dyn AnyActor + Send> },
    Spawn { tag: String, actor: Box<dyn AnyActor + Send> },
    Queue { tag: String, queue: Vec<Envelope> },
    Delay { entry: Entry },
    Stop { tag: String },
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

#[derive(Default)]
struct SchedulerMetrics {
    at: u64,
    millis: u64,
    miss: u64,
    hit: u64,
    tick: u64,
    messages: u64,
    queues: u64,
    returns: u64,
    spawns: u64,
    delays: u64,
}

impl fmt::Debug for SchedulerMetrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SchedulerMetrics")
            .field("at", &self.at)
            .field("millis", &self.millis)
            .field("miss", &self.miss)
            .field("hit", &self.hit)
            .field("tick", &self.tick)
            .field("messages", &self.messages)
            .field("queues", &self.queues)
            .field("returns", &self.returns)
            .field("spawns", &self.spawns)
            .field("delays", &self.delays)
            .finish()
    }
}

#[derive(Copy, Clone)]
pub struct SchedulerConfig {
    // Maximum number of envelopes an actor can process at single scheduled execution
    throughput: usize,
    metric_reporting_interval: Duration,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        SchedulerConfig {
            throughput: 1,
            metric_reporting_interval: Duration::from_secs(1),
        }
    }
}

// TODO return subscription/handle instead of blocking forever
pub fn start_actor_runtime(mut scheduler: Scheduler, mut pool: ThreadPool, config_opt: Option<SchedulerConfig>) {
    let config = config_opt.unwrap_or_default();

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
            let mut memory: Memory<Envelope> = Memory::default();
            loop {
                let event = rx.lock().unwrap().try_recv();
                if let Ok(x) = event {
                    match x {
                        Event::Mail { tag, mut actor, mut queue } => {
                            if queue.len() > config.throughput {
                                let remaining = queue.split_off(config.throughput);
                                tx.send(Action::Queue { tag: tag.clone(), queue: remaining }).unwrap();
                            }
                            for envelope in queue.into_iter() {
                                actor.receive(envelope, &mut memory);
                            }
                            tx.send(Action::Return { tag, actor }).unwrap();
                        }
                    }
                    memory.drain(&tx);
                }
            }
        });
    }

    pool.submit(move || {
        let mut metrics = SchedulerMetrics::default();
        let mut start = Instant::now();
        loop {
            let received = actions_rx.try_recv();
            if let Ok(action) = received {
                metrics.hit += 1;
                match action {
                    Action::Return { tag, actor } => {
                        if scheduler.active.contains(&tag) {
                            metrics.returns += 1;
                            let mut queue = scheduler.queue.remove(&tag).unwrap_or_default();
                            if !queue.is_empty() {
                                if queue.len() > config.throughput {
                                    let remaining = queue.split_off(config.throughput);
                                    scheduler.queue.insert(tag.clone(), remaining);
                                }
                                actions_tx.send(Action::Queue { tag: tag.clone(), queue }).unwrap();
                            }
                            scheduler.actors.insert(tag, actor);
                        }
                    },
                    Action::Queue { tag, queue } => {
                        if scheduler.active.contains(&tag) {
                            metrics.queues += 1;
                            metrics.messages += queue.len() as u64;
                            match scheduler.actors.remove(&tag) {
                                Some(actor) => {
                                    let event = Event::Mail { tag, actor, queue };
                                    events_tx.send(event).unwrap();
                                },
                                None => {
                                    scheduler.queue
                                        .entry(tag.clone())
                                        .or_default()
                                        .extend(queue.into_iter());
                                }
                            }
                        }
                    },
                    Action::Spawn { tag, actor } => {
                        if !scheduler.active.contains(&tag) {
                            metrics.spawns += 1;
                            scheduler.actors.insert(tag.clone(), actor);
                            scheduler.active.insert(tag);
                        }
                    },
                    Action::Delay { entry } => {
                        metrics.delays +=1 ;
                        scheduler.tasks.push(entry);
                    },
                    Action::Stop { tag } => {
                        scheduler.stop(&tag);
                    }
                }
            } else {
                metrics.miss +=1 ;
            }

            // TODO Collect statistic about running time of this loop to estimate `precision`
            let precision = Duration::from_millis(1);
            let now = Instant::now().add(precision);
            while scheduler.tasks.peek().map(|e| e.at <= now).unwrap_or_default() {
                if let Some(Entry { tag, envelope, .. }) = scheduler.tasks.pop() {
                    let action = Action::Queue { tag, queue: vec![envelope] };
                    actions_tx.send(action).unwrap();
                }
            }

            metrics.tick += 1;
            if start.elapsed() >= config.metric_reporting_interval {
                let now = SystemTime::now();
                metrics.at = now.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
                metrics.millis = start.elapsed().as_millis() as u64;
                println!("{:?}", metrics);
                metrics = SchedulerMetrics::default();
                start = Instant::now();
            }
        }
    });
}
