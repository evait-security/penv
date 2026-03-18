use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

#[derive(Parser)]
#[command(
    name = "penv",
    about = "Pentester Environment – manage network and customer-specific environment variables",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Read current.yaml and output export commands for eval
    Init,

    /// Auto-discover network info (IP, gateway, DNS, domain) and save to current.yaml
    Discover,

    /// Add or update a variable in current.yaml
    Set {
        /// Variable name (e.g. user, password, ip)
        key: String,
        /// Value to assign
        value: String,
    },

    /// Remove a variable from current.yaml
    Unset {
        /// Variable name to remove
        key: String,
    },

    /// Print all currently active variables
    List,

    /// Save current state as a named profile
    Store {
        /// Profile name (stored as ~/.local/penv/<name>.yaml)
        profile_name: String,
    },

    /// Load a named profile into current.yaml
    Load {
        /// Profile name to load
        profile_name: String,
    },

    /// Generate shell completion script
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

pub fn print_completions(shell: Shell) {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut std::io::stdout());
}
