//! Subcommands for `gglib benchmark`.

use clap::Subcommand;

/// Subcommands available under `gglib benchmark`.
#[derive(Subcommand)]
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
}
