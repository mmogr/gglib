#![doc = include_str!("README.md")]

pub mod agentic;
pub mod compare;
pub mod events;
pub mod perf;
pub mod run;
pub mod summary;
pub mod tune;

pub use agentic::{
    AgenticEvalConfig, AgenticEvalReport, AgenticTaskComparison, ArmDelta, ArmScores,
    CONTROL_MIN_COMPOSITE_GAP, CONTROL_MIN_P, CONTROL_TEMPERATURE, CONTROL_TOP_K, CONTROL_TOP_P,
    ControlVerdict, DEFAULT_SEEDS, EvalArm, control_sampling,
};
pub use compare::{CompareConfig, ModelCompareResult};
pub use events::{BenchmarkEvent, BenchmarkModelResult};
pub use perf::{ModelPerfResult, PerfConfig};
pub use run::{BenchmarkRun, BenchmarkRunStatus, BenchmarkRunType};
pub use summary::ModelBenchmarkSummary;
pub use tune::{
    CandidateSource, ScoreWeights, SweepSpec, TaskCategory, TaskSuite, TuneCandidateResult,
    TuneConfig, TuneTask, TuneTaskResult,
};
