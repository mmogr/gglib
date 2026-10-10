//! stable-diffusion.cpp management subcommands.
//!
//! The image runtime: `sd-server`, which serves the models that draw. The
//! twin of [`LlamaCommand`](crate::llama_commands::LlamaCommand) for the
//! three verbs the image runtime has: it has no update or rebuild of its
//! own, since reinstalling (`--force`) is both.

use clap::Subcommand;

/// stable-diffusion.cpp management commands.
#[derive(Subcommand)]
pub enum SdCommand {
    /// Install stable-diffusion.cpp's sd-server, which serves image models
    Install {
        /// Reinstall even if already installed
        #[arg(short, long)]
        force: bool,
        /// Force building from source instead of downloading pre-built binaries
        #[arg(long)]
        build: bool,
    },

    /// Show what is installed for image generation
    Status,

    /// Remove stable-diffusion.cpp's installation
    Uninstall {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::SdCommand;
    use crate::commands::Commands;
    use crate::config_commands::ConfigCommand;
    use clap::Parser;
    use gglib_core::paths::SD_INSTALL_COMMAND;

    /// The `sd` subcommand `argv` parses to.
    fn parsed(argv: &[&str]) -> SdCommand {
        let cli = crate::Cli::try_parse_from(argv)
            .unwrap_or_else(|e| panic!("{argv:?} should parse: {e}"));
        match cli.command {
            Some(Commands::Config {
                command: ConfigCommand::Sd { command },
            }) => command,
            _ => panic!("{argv:?} is not a `config sd` command"),
        }
    }

    /// The proxy's `image_runtime_not_installed` refusal, the web page and
    /// `gglib serve` tell the user to run this constant, so it has to be a
    /// command this parser accepts.
    #[test]
    fn the_install_hint_is_the_command_that_installs_sd() {
        let argv: Vec<&str> = SD_INSTALL_COMMAND.split(' ').collect();
        assert!(matches!(
            parsed(&argv),
            SdCommand::Install {
                force: false,
                build: false
            }
        ));
    }

    #[test]
    fn install_takes_force_and_build() {
        assert!(matches!(
            parsed(&["gglib", "config", "sd", "install", "--force", "--build"]),
            SdCommand::Install {
                force: true,
                build: true
            }
        ));
        assert!(matches!(
            parsed(&["gglib", "config", "sd", "install", "-f"]),
            SdCommand::Install {
                force: true,
                build: false
            }
        ));
    }

    #[test]
    fn status_parses() {
        assert!(matches!(
            parsed(&["gglib", "config", "sd", "status"]),
            SdCommand::Status
        ));
    }

    #[test]
    fn uninstall_asks_unless_forced() {
        assert!(matches!(
            parsed(&["gglib", "config", "sd", "uninstall"]),
            SdCommand::Uninstall { force: false }
        ));
        assert!(matches!(
            parsed(&["gglib", "config", "sd", "uninstall", "--force"]),
            SdCommand::Uninstall { force: true }
        ));
    }

    /// The source build has no acceleration flags of its own: it builds for
    /// what the machine has, as a first install's offer does.
    #[test]
    fn install_takes_no_acceleration_flag() {
        assert!(
            crate::Cli::try_parse_from(["gglib", "config", "sd", "install", "--cuda"]).is_err()
        );
    }
}
