#[macro_use]
extern crate bencher;
use bencher::Bencher;

#[path = "../src/pool.rs"]
mod pool;

#[path = "../src/core.rs"]
mod core;

use crate::core::{System, Config, AnyActor, AnySender, Envelope};
use std::sync::mpsc::{Sender, channel};

fn minimum(b: &mut Bencher) {
    b.iter(|| {
        let cfg = Config::default();
        let sys = System::new(cfg);
        let run = sys.run();
        run.shutdown();
    });
}

#[derive(Default)]
struct Test {
    count: usize,
    limit: usize,
    tx: Option<Sender<usize>>,
}

enum Protocol {
    Init(usize, Sender<usize>),
    Hit,
}

impl AnyActor for Test {
    fn receive(&mut self, envelope: Envelope, sender: &mut dyn AnySender) {
        if let Some(p) = envelope.message.downcast_ref::<Protocol>() {
            let me = sender.myself();
            match p {
                Protocol::Init(limit, tx) => {
                    self.limit = *limit;
                    self.tx = Some(tx.to_owned());
                    sender.send(&me, Envelope::of(Protocol::Hit, &me));
                },
                Protocol::Hit if self.count < self.limit => {
                    self.count += 1;
                    sender.send(&me, Envelope::of(Protocol::Hit, &me));
                },
                Protocol::Hit => {
                    self.tx.take().unwrap().send(self.count).unwrap();
                    sender.stop(&me);
                }
            }
        }
    }
}

fn counter(b: &mut Bencher) {
    b.iter(|| {
        let cfg = Config::default();
        let sys = System::new(cfg);
        let run = sys.run();

        let (tx, rx) = channel();
        run.spawn_default::<Test>("test");
        run.send("test", Envelope::of(Protocol::Init(1000, tx), ""));

        rx.recv().unwrap();

        run.shutdown();
    });
}

benchmark_group!(benches, minimum, counter);
benchmark_main!(benches);
