//! Subcommands for `gglib benchmark`.

use clap::Subcommand;

/// Subcommands available under `gglib benchmark`.
#[derive(Clone, Subcommand)]
pub enum BenchmarkCommand {
    /// Run the same prompt through multiple models and compare outputs side-by-side
    #[command(display_order = 1)]
    Compare {
        /// Prompt to send to every model
        #[arg(long, short = 'p')]
        prompt: String,

        /// Model name or database ID (repeat for multiple models)
        #[arg(long = "model", short = 'm', required = true)]
        models: Vec<String>,

        /// Optional system prompt
        #[arg(long = "system", short = 's')]
        system_prompt: Option<String>,

        /// Sampling temperature override (0.0 – 2.0)
        #[arg(long)]
        temperature: Option<f32>,

        /// Maximum tokens to generate per model
        #[arg(long)]
        max_tokens: Option<u32>,

        /// Context size override (number of tokens, or `max`)
        #[arg(long)]
        ctx_size: Option<u64>,
    },

    /// Measure raw prompt-processing and token-generation throughput with llama-bench
    #[command(display_order = 2)]
    Perf {
        /// Model name or database ID (repeat for multiple models)
        #[arg(long = "model", short = 'm', required = true)]
        models: Vec<String>,

        /// Number of prompt tokens to use in the benchmark
        #[arg(long, default_value = "512")]
        pp: u32,

        /// Number of generation tokens to use in the benchmark
        #[arg(long, default_value = "128")]
        tg: u32,

        /// Number of repetitions to average over
        #[arg(long, default_value = "3")]
        reps: u32,
    },

    /// List past benchmark runs
    #[command(display_order = 3)]
    List {
        /// Maximum number of runs to show
        #[arg(long, short = 'n', default_value = "10")]
        limit: i64,
    },

    /// Show the details of a specific benchmark run
    #[command(display_order = 4)]
    Show {
        /// ID of the run to inspect
        run_id: i64,
    },

    /// Show the full benchmark history (compare + perf) for one model
    #[command(display_order = 5)]
    Model {
        /// Database ID of the model to show history for
        model_id: i64,
    },

    /// Sweep sampling parameters for one model against an agentic
    /// tool-calling task suite to find the settings that make it both
    /// accurate at tool calls and resistant to loop/stagnation
    #[command(display_order = 6)]
    Tune {
        /// Model name or database ID to tune (exactly one)
        #[arg(long = "model", short = 'm')]
        model: String,

        /// Sweep a sampling parameter across candidate values, e.g.
        /// `--sweep temperature=0.2,0.5,0.8`. Repeatable — one flag per
        /// dimension (`temperature`, `top_p`, `top_k`, `min_p`,
        /// `repeat_penalty`, `dry_multiplier`). A dimension left unswept is
        /// not varied; the normal per-model/global/hardcoded fallback chain
        /// fills it in. The grid is the cartesian product, so dimensions
        /// multiply.
        #[arg(long = "sweep", value_name = "DIM=V1,V2,...")]
        sweep: Vec<String>,

        /// Task suite to evaluate candidates against: `default` for the
        /// built-in BFCL-style suite (single-call, parallel-call,
        /// multi-turn, irrelevance-detection, long-context-endurance), or
        /// a path to a custom JSON file containing an array of task
        /// definitions in the same schema as the built-in suite (see
        /// `gglib-core/assets/tune_default_suite.json` for the shape to
        /// copy). The GUI accepts the identical array shape from a file
        /// upload — there is one shared task schema for both surfaces.
        #[arg(long = "task-suite", default_value = "default")]
        task_suite: String,

        /// Seed the candidate grid with the model's GGUF metadata
        /// author-recommended sampling defaults, when present (no-op
        /// today — no GGUF metadata convention for this exists yet).
        /// Pass `--seed-from-gguf false` to disable
        #[arg(long, action = clap::ArgAction::Set, default_value_t = true)]
        seed_from_gguf: bool,

        /// Seed the candidate grid with built-in per-model-family sampling
        /// presets (e.g. Qwen coding-mode defaults), matched by a
        /// case-insensitive substring of the model's name. Pass
        /// `--seed-from-family-presets false` to disable
        #[arg(long, action = clap::ArgAction::Set, default_value_t = true)]
        seed_from_family_presets: bool,

        /// Fraction of candidates dropped after the cheap pre-screen round
        /// (one `single_call` + one `irrelevance` task). Clamped to
        /// `[0.0, 0.9]`; a floor of 3 survivors always applies regardless
        /// of how aggressive this is set
        #[arg(long, default_value = "0.5")]
        prune_fraction: f32,

        /// Composite-score weight for average tool-call match accuracy
        #[arg(long)]
        weight_tool_accuracy: Option<f32>,

        /// Composite-score weight for `1 - (loop/stagnation trigger rate)`
        #[arg(long)]
        weight_loop_avoidance: Option<f32>,

        /// Composite-score weight for the fraction of tasks completed
        /// (produced a final answer instead of hitting a loop/stagnation
        /// guard or the iteration ceiling)
        #[arg(long)]
        weight_task_completion: Option<f32>,

        /// Composite-score weight for token-generation throughput.
        /// `tg_tps` is measured and reported per candidate, but this weight
        /// does not yet enter the composite (its definition is relative to
        /// the fastest candidate, which is unknowable mid-stream — see the
        /// tune service's module docs)
        #[arg(long)]
        weight_speed: Option<f32>,

        /// Context size override (number of tokens)
        #[arg(long)]
        ctx_size: Option<u64>,

        /// After the run completes, write the highest-scoring candidate's
        /// sampling settings to this model's `inference_defaults` — the
        /// same effect as `gglib model update <id> --temperature ...`,
        /// applied automatically
        #[arg(long)]
        apply_best: bool,
    },

    /// Measure what the gglib pipeline buys: run the agentic task suite
    /// twice against one model — once with the pipeline bypassed (bare
    /// llama-server defaults) and once through it — and report the
    /// before/after delta in tool-call accuracy, loop avoidance, and task
    /// completion
    #[command(display_order = 7)]
    Agentic {
        /// Model name or database ID to evaluate (exactly one)
        #[arg(long = "model", short = 'm')]
        model: String,

        /// Task suite both arms run: `default` for the built-in BFCL-style
        /// suite, or a path to a custom JSON task file (same schema as
        /// `gglib benchmark tune --task-suite`)
        #[arg(long = "task-suite", default_value = "default")]
        task_suite: String,

        /// Context size override (number of tokens)
        #[arg(long)]
        ctx_size: Option<u64>,

        /// Print the full report as JSON to stdout (the leaderboard
        /// interchange format: per-arm scores, deltas, per-task drill-down,
        /// model/quant identity, hardware snapshot)
        #[arg(long)]
        json: bool,

        /// Write the same JSON report to this file
        #[arg(long, value_name = "FILE")]
        output: Option<std::path::PathBuf>,
    },
}
