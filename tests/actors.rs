#[path = "../src/pool.rs"]
mod pool;

#[path = "../src/core.rs"]
mod core;

use crate::core::{AnyActor, AnySender, Envelope, System, Config, Run};
use std::sync::mpsc::{channel, Sender, RecvTimeoutError};
use std::fmt::Debug;
use std::time::Duration;
use std::ops::Add;

const ANSWER: usize = 42;

struct Message(Sender<usize>);

struct Test(usize);

impl AnyActor for Test {
    fn receive(&mut self, envelope: Envelope, _sender: &mut dyn AnySender) {
        if let Some(message) = envelope.message.downcast_ref::<Message>() {
            message.0.send(self.0).unwrap();
        }
    }
}

struct Proxy { target: String }

impl AnyActor for Proxy {
    fn receive(&mut self, envelope: Envelope, sender: &mut dyn AnySender) {
        sender.send(&self.target, envelope);
    }
}

struct Counter(usize, Sender<usize>);

enum CounterProtocol {
    Inc,
    Get
}

impl AnyActor for Counter {
    fn receive(&mut self, envelope: Envelope, sender: &mut dyn AnySender) {
        if let Some(p) = envelope.message.downcast_ref::<CounterProtocol>() {
            match p {
                CounterProtocol::Inc => {
                    self.0 += 1;
                    self.1.send(self.0).unwrap();
                },
                CounterProtocol::Get => {
                    let env = Envelope { message: Box::new(CounterProtocol::Inc), from: String::default() };
                    sender.send(&sender.myself(), env);
                }
            }
        }
    }
}

fn with_run<T: Eq + Debug, E, F: FnOnce(&Run) -> Result<T, E>>(expected: T, f: F) -> Result<(), E> {
    let cfg = Config::default();
    let sys = System::new(cfg);
    let run = sys.run();
    let got = f(&run);
    run.shutdown();
    let actual = got?;
    assert_eq!(actual, expected);
    Ok(())
}

const TIMEOUT: Duration = Duration::from_millis(200);

#[test]
fn sent_message_received() -> Result<(), RecvTimeoutError> {
    with_run(ANSWER, |run| {
        run.spawn("test", || Box::new(Test(ANSWER)));

        let (tx, rx) = channel();
        let env = Envelope { message: Box::new(Message(tx)), from: String::default() };
        run.send("test", env);

        rx.recv_timeout(TIMEOUT)
    })
}

#[test]
fn forwarded_message_received() -> Result<(), RecvTimeoutError> {
    with_run(ANSWER, |run| {
        run.spawn("test", || Box::new(Test(ANSWER)));
        run.spawn("proxy", || Box::new(Proxy { target: "test".to_string() }));

        let (tx, rx) = channel();
        let env = Envelope { message: Box::new(Message(tx)), from: String::default() };
        run.send("proxy", env);

        rx.recv_timeout(TIMEOUT)
    })
}

#[test]
fn delayed_message_received() -> Result<(), RecvTimeoutError> {
    with_run(ANSWER, |run| {
        run.spawn("test", || Box::new(Test(ANSWER)));

        let (tx, rx) = channel();
        let env = Envelope { message: Box::new(Message(tx)), from: String::default() };

        const DELAY: Duration = Duration::from_millis(100);
        run.delay("test", env, DELAY);

        rx.recv_timeout(TIMEOUT.add(DELAY))
    })
}

#[test]
fn own_message_received() -> Result<(), RecvTimeoutError> {
    with_run(ANSWER + 1, |run| {
        let (tx, rx) = channel();
        run.spawn("test", || Box::new(Counter(ANSWER, tx)));

        let env = Envelope { message: Box::new(CounterProtocol::Get), from: String::default() };
        run.send("test", env);

        rx.recv_timeout(TIMEOUT)
    })
}