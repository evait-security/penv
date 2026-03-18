mod cli;
mod config;
mod network;

use std::fs;

use clap::Parser;
use cli::{Cli, Commands};
use config::Config;

fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Commands::Init => cmd_init(),
        Commands::Discover => cmd_discover(),
        Commands::Set { key, value } => cmd_set(&key, &value),
        Commands::Unset { key } => cmd_unset(&key),
        Commands::List => cmd_list(),
        Commands::Store { profile_name } => cmd_store(&profile_name),
        Commands::Load { profile_name } => cmd_load(&profile_name),
        Commands::Completions { shell } => {
            cli::print_completions(shell);
            Ok(())
        }
        Commands::ShellInit { shell } => cmd_shell_init(shell),
    }
}

// ---------------------------------------------------------------------------
// penv init
// ---------------------------------------------------------------------------

fn cmd_init() -> anyhow::Result<()> {
    let path = Config::current_path()?;
    let cfg = Config::load(&path)?;
    for (key, value) in &cfg.vars {
        // Sanitize: reject keys containing shell metacharacters so that the
        // output is safe for `eval`.
        if !is_safe_key(key) {
            eprintln!("penv: skipping unsafe variable name: {key}");
            continue;
        }
        // Single-quote the value and escape any embedded single quotes.
        let safe_value = shell_single_quote(value);
        println!("export {key}={safe_value}");
    }
    Ok(())
}

/// Returns true if the key consists only of alphanumeric characters and
/// underscores and does not start with a digit – i.e. a valid shell variable
/// name.
fn is_safe_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        None => false,
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

/// Wrap a value in single quotes, escaping any embedded single quotes.
/// e.g. `it's` -> `'it'"'"'s'`
fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

// ---------------------------------------------------------------------------
// penv discover
// ---------------------------------------------------------------------------

fn cmd_discover() -> anyhow::Result<()> {
    let path = Config::current_path()?;
    let mut cfg = Config::load(&path)?;

    let info = network::discover();

    if let Some(ip) = info.ip {
        println!("ip       = {ip}");
        cfg.vars.insert("ip".to_string(), ip);
    } else {
        eprintln!("penv: could not determine local IP address");
    }

    if let Some(gw) = info.gateway {
        println!("gateway  = {gw}");
        cfg.vars.insert("gateway".to_string(), gw);
    } else {
        eprintln!("penv: could not determine default gateway");
    }

    if let Some(dns) = info.dns {
        println!("dc       = {dns}");
        cfg.vars.insert("dc".to_string(), dns);
    } else {
        eprintln!("penv: could not determine DNS/DC server");
    }

    if let Some(domain) = info.domain {
        println!("domain   = {domain}");
        cfg.vars.insert("domain".to_string(), domain);
    } else {
        eprintln!("penv: could not determine domain name");
    }

    cfg.save(&path)?;
    println!("\nSaved to {}", path.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// penv set
// ---------------------------------------------------------------------------

fn cmd_set(key: &str, value: &str) -> anyhow::Result<()> {
    if !is_safe_key(key) {
        anyhow::bail!(
            "'{key}' is not a valid shell variable name. \
             Use only letters, digits, and underscores, starting with a letter or underscore."
        );
    }
    let path = Config::current_path()?;
    let mut cfg = Config::load(&path)?;
    cfg.vars.insert(key.to_string(), value.to_string());
    cfg.save(&path)?;
    println!("Set {key} = {value}");
    Ok(())
}

// ---------------------------------------------------------------------------
// penv unset
// ---------------------------------------------------------------------------

fn cmd_unset(key: &str) -> anyhow::Result<()> {
    let path = Config::current_path()?;
    let mut cfg = Config::load(&path)?;
    if cfg.vars.remove(key).is_some() {
        cfg.save(&path)?;
        println!("Removed {key}");
    } else {
        eprintln!("penv: variable '{key}' not found");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// penv list
// ---------------------------------------------------------------------------

fn cmd_list() -> anyhow::Result<()> {
    let path = Config::current_path()?;
    let cfg = Config::load(&path)?;
    if cfg.vars.is_empty() {
        println!("No variables set. Run `penv discover` or `penv set <key> <value>`.");
        return Ok(());
    }
    let max_key_len = cfg.vars.keys().map(|k| k.len()).max().unwrap_or(0);
    for (key, value) in &cfg.vars {
        println!("{key:<max_key_len$} = {value}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// penv store
// ---------------------------------------------------------------------------

fn cmd_store(profile_name: &str) -> anyhow::Result<()> {
    validate_profile_name(profile_name)?;
    let src = Config::current_path()?;
    let dst = Config::profile_path(profile_name)?;
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&src, &dst)?;
    println!("Profile '{profile_name}' saved to {}", dst.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// penv load
// ---------------------------------------------------------------------------

fn cmd_load(profile_name: &str) -> anyhow::Result<()> {
    validate_profile_name(profile_name)?;
    let src = Config::profile_path(profile_name)?;
    if !src.exists() {
        anyhow::bail!("Profile '{profile_name}' not found at {}", src.display());
    }
    let dst = Config::current_path()?;
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&src, &dst)?;
    println!("Profile '{profile_name}' loaded.");
    Ok(())
}

// ---------------------------------------------------------------------------
// penv shell-init
// ---------------------------------------------------------------------------

fn cmd_shell_init(shell: Option<clap_complete::Shell>) -> anyhow::Result<()> {
    use clap_complete::Shell;

    let shell = shell.unwrap_or_else(detect_shell);

    match shell {
        Shell::Bash => print!("{}", SHELL_INIT_BASH),
        Shell::Zsh => print!("{}", SHELL_INIT_ZSH),
        Shell::Fish => print!("{}", SHELL_INIT_FISH),
        _ => anyhow::bail!("Unsupported shell. Use bash, zsh, or fish."),
    }

    Ok(())
}

fn detect_shell() -> clap_complete::Shell {
    use clap_complete::Shell;

    if let Ok(shell) = std::env::var("SHELL") {
        if shell.ends_with("/fish") {
            return Shell::Fish;
        }
        if shell.ends_with("/zsh") {
            return Shell::Zsh;
        }
    }
    Shell::Bash
}

const SHELL_INIT_BASH: &str = r#"# penv shell integration
# Wrapper function that auto-reloads environment after modifying commands
penv() {
    local cmd="$1"
    command penv "$@"
    local ret=$?
    if [[ $ret -eq 0 ]]; then
        case "$cmd" in
            set|unset|load|discover)
                eval "$(command penv init)"
                ;;
        esac
    fi
    return $ret
}

# Initial load
eval "$(command penv init)"
"#;

const SHELL_INIT_ZSH: &str = r#"# penv shell integration
# Wrapper function that auto-reloads environment after modifying commands
penv() {
    local cmd="$1"
    command penv "$@"
    local ret=$?
    if [[ $ret -eq 0 ]]; then
        case "$cmd" in
            set|unset|load|discover)
                eval "$(command penv init)"
                ;;
        esac
    fi
    return $ret
}

# Initial load
eval "$(command penv init)"
"#;

const SHELL_INIT_FISH: &str = r#"# penv shell integration
# Wrapper function that auto-reloads environment after modifying commands
function penv
    set -l cmd $argv[1]
    command penv $argv
    set -l ret $status
    if test $ret -eq 0
        switch "$cmd"
            case set unset load discover
                eval (command penv init | string replace -a 'export ' 'set -gx ' | string replace -a '=' ' ')
        end
    end
    return $ret
end

# Initial load
eval (command penv init | string replace -a 'export ' 'set -gx ' | string replace -a '=' ' ')
"#;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Reject profile names that could be used for path traversal.
fn validate_profile_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty()
        || name == "current"
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
    {
        anyhow::bail!(
            "'{name}' is not a valid profile name. \
             Use alphanumeric characters and hyphens/underscores only, \
             and avoid the reserved name 'current'."
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_safe_key_valid() {
        assert!(is_safe_key("ip"));
        assert!(is_safe_key("dc"));
        assert!(is_safe_key("domain"));
        assert!(is_safe_key("_private"));
        assert!(is_safe_key("user123"));
    }

    #[test]
    fn test_is_safe_key_invalid() {
        assert!(!is_safe_key(""));
        assert!(!is_safe_key("123start"));
        assert!(!is_safe_key("bad-name"));
        assert!(!is_safe_key("bad name"));
        assert!(!is_safe_key("bad;name"));
        assert!(!is_safe_key("$(cmd)"));
    }

    #[test]
    fn test_shell_single_quote_plain() {
        assert_eq!(shell_single_quote("hello"), "'hello'");
    }

    #[test]
    fn test_shell_single_quote_with_single_quote() {
        assert_eq!(shell_single_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn test_shell_single_quote_special_chars() {
        assert_eq!(shell_single_quote("P@ssw0rd!"), "'P@ssw0rd!'");
        assert_eq!(shell_single_quote("a$b`c"), "'a$b`c'");
    }

    #[test]
    fn test_validate_profile_name_valid() {
        assert!(validate_profile_name("customer_1").is_ok());
        assert!(validate_profile_name("acme-corp").is_ok());
        assert!(validate_profile_name("test123").is_ok());
    }

    #[test]
    fn test_validate_profile_name_invalid() {
        assert!(validate_profile_name("").is_err());
        assert!(validate_profile_name("current").is_err());
        assert!(validate_profile_name("../../etc/passwd").is_err());
        assert!(validate_profile_name("path/traversal").is_err());
        assert!(validate_profile_name("back\\slash").is_err());
    }
}
