use std::time::Duration;

#[derive(Default)]
pub struct Config {
    pub(crate) runtime: RuntimeConfig,
    pub(crate) scheduler: SchedulerConfig,
}

impl Config {
    pub fn new(runtime: RuntimeConfig, scheduler: SchedulerConfig) -> Config {
        Config {
            runtime,
            scheduler,
        }
    }
}

#[derive(Copy, Clone)]
pub struct SchedulerConfig {
    // Maximum number of envelopes an actor can process at single scheduled execution
    pub(crate) throughput: usize,
    pub(crate) metric_reporting_interval: Duration,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        SchedulerConfig {
            throughput: 1,
            metric_reporting_interval: Duration::from_secs(1),
        }
    }
}

pub struct RuntimeConfig {
    pub(crate) threads: usize,
}

impl RuntimeConfig {
    pub fn with_threads(threads: usize) -> RuntimeConfig {
        RuntimeConfig {
            threads,
        }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        RuntimeConfig {
            threads: 4,
        }
    }
}
