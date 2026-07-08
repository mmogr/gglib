#![doc = include_str!("README.md")]

pub mod compare;
pub mod events;
pub mod perf;
pub mod run;
pub mod summary;
pub mod tune;

pub use compare::{CompareConfig, ModelCompareResult};
pub use events::{BenchmarkEvent, BenchmarkModelResult};
pub use perf::{ModelPerfResult, PerfConfig};
pub use run::{BenchmarkRun, BenchmarkRunStatus, BenchmarkRunType};
pub use summary::ModelBenchmarkSummary;
pub use tune::{ScoreWeights, SweepSpec, TaskCategory, TaskSuite, TuneConfig, TuneTask};
