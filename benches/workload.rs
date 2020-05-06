#[macro_use]
extern crate bencher;
use bencher::Bencher;

#[path = "../src/pool.rs"]
mod pool;

#[path = "../src/core.rs"]
mod core;

use crate::core::{System, Config};

fn start_stop(b: &mut Bencher) {
    b.iter(|| {
        let cfg = Config::default();
        let sys = System::new(cfg);
        let sub = sys.run();
        sub.shutdown();
    });
}

benchmark_group!(benches, start_stop);
benchmark_main!(benches);
