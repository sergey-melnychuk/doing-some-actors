use std::any::Any;
use std::collections::{HashMap, BinaryHeap};

use crate::pool::ThreadPool;
use std::sync::mpsc::channel;
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
            let mut memory: Memory<Envelope> = Memory::new();
            loop {
                let event = rx.lock().unwrap().try_recv();
                if let Ok(x) = event {
                    match x {
                        Event::Mail { tag, mut actor, mut queue } => {
                            //println!("[worker thread] event.mail for tag '{}': {} messages", id, tag, queue.len());
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
        let mut metrics = SchedulerMetrics::default();
        let mut start = Instant::now();
        loop {
            let received = actions_rx.try_recv();
            if let Ok(action) = received {
                metrics.hit += 1;
                match action {
                    Action::Return { tag, actor } => {
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
                    },
                    Action::Queue { tag, queue } => {
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
                    },
                    Action::Spawn { tag, actor } => {
                        metrics.spawns += 1;
                        scheduler.actors.insert(tag, actor);
                    },
                    Action::Delay { entry } => {
                        metrics.delays +=1 ;
                        scheduler.tasks.push(entry);
                    }
                }
            } else {
                metrics.miss +=1 ;
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
