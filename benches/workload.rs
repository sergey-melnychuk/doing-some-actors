#[macro_use]
extern crate bencher;
use bencher::Bencher;

#[path = "../src/pool.rs"]
mod pool;

#[path = "../src/core.rs"]
mod core;

use crate::core::{System, Config};

fn minimum(b: &mut Bencher) {
    b.iter(|| {
        let cfg = Config::default();
        let sys = System::new(cfg);
        let run = sys.run();
        run.shutdown();
    });
}

benchmark_group!(benches, minimum);
benchmark_main!(benches);
