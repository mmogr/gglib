//! llama.cpp management subcommands.
//!
//! This module defines the llama.cpp installation and management commands.

use clap::Subcommand;

/// llama.cpp management commands.
///
/// The same verbs as stable-diffusion.cpp's (`SdCommand`), plus `detect`:
/// `install` installs the pinned release when what is there is missing or
/// older, which is also what `update` does.
#[derive(Subcommand)]
pub enum LlamaCommand {
    /// Install the pinned llama.cpp release, or update to it
    #[command(visible_alias = "update")]
    Install {
        /// Reinstall even when the pinned release is installed. Over a source
        /// build, takes the download where one fits
        #[arg(short, long)]
        force: bool,
        /// Build from source instead of downloading pre-built binaries
        #[arg(long)]
        build: bool,
    },

    /// Show what is installed, and where it stands against the pinned release
    Status,

    /// Remove llama.cpp installation
    Uninstall {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },

    /// Detect GPU acceleration and check build readiness
    Detect {
        /// Output raw JSON instead of human-readable summary
        #[arg(long)]
        json: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::LlamaCommand;
    use crate::commands::Commands;
    use crate::config_commands::ConfigCommand;
    use clap::Parser;
    use gglib_core::paths::LLAMA_INSTALL_COMMAND;

    /// The GUI's install prompt and the benchmark's missing-binary message
    /// tell the user to run this constant, so it has to be a command this
    /// parser accepts.
    #[test]
    fn the_install_hint_is_the_command_that_installs_llama() {
        let cli = crate::Cli::try_parse_from(LLAMA_INSTALL_COMMAND.split(' '))
            .unwrap_or_else(|e| panic!("{LLAMA_INSTALL_COMMAND:?} should parse: {e}"));

        assert!(matches!(
            cli.command,
            Some(Commands::Config {
                command: ConfigCommand::Llama {
                    command: LlamaCommand::Install { .. }
                }
            })
        ));
    }

    fn parsed(argv: &[&str]) -> LlamaCommand {
        let cli = crate::Cli::try_parse_from(argv)
            .unwrap_or_else(|e| panic!("{argv:?} should parse: {e}"));
        match cli.command {
            Some(Commands::Config {
                command: ConfigCommand::Llama { command },
            }) => command,
            _ => panic!("{argv:?} is not a `config llama` command"),
        }
    }

    /// `update` is `install`: both install the pinned release when what is
    /// there is missing or older.
    #[test]
    fn update_is_install_with_the_same_flags() {
        assert!(matches!(
            parsed(&["gglib", "config", "llama", "update"]),
            LlamaCommand::Install {
                force: false,
                build: false
            }
        ));
        assert!(matches!(
            parsed(&["gglib", "config", "llama", "update", "--force", "--build"]),
            LlamaCommand::Install {
                force: true,
                build: true
            }
        ));
    }

    /// The acceleration is detected, never chosen, and the verbs the pin made
    /// redundant are gone.
    #[test]
    fn the_flags_and_verbs_the_pin_replaced_are_refused() {
        for argv in [
            &["gglib", "config", "llama", "install", "--cuda"][..],
            &["gglib", "config", "llama", "install", "--metal"],
            &["gglib", "config", "llama", "install", "--vulkan"],
            &["gglib", "config", "llama", "rebuild"],
            &["gglib", "config", "llama", "check-updates"],
        ] {
            assert!(crate::Cli::try_parse_from(argv).is_err(), "{argv:?}");
        }
    }
}
