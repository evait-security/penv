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
        Commands::Discover { dry_run, json } => cmd_discover(dry_run, json),
        Commands::Set { key, value } => cmd_set(&key, &value),
        Commands::Unset { key } => cmd_unset(&key),
        Commands::Print { profile_name } => cmd_print(profile_name.as_deref()),
        Commands::List => cmd_list(),
        Commands::Clean => cmd_clean(),
        Commands::Store { profile_name } => cmd_store(&profile_name),
        Commands::Load { profile_name } => cmd_load(&profile_name),
        Commands::Drop { profile_name } => cmd_drop(&profile_name),
        Commands::Completions { shell } => {
            cli::print_completions(shell);
            Ok(())
        }
        Commands::ShellInit { shell, autocomplete } => cmd_shell_init(shell, autocomplete),
        Commands::ListProfiles => cmd_list_profiles(),
        Commands::ListVars => cmd_list_vars(),
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

fn cmd_discover(dry_run: bool, json: bool) -> anyhow::Result<()> {
    let path = Config::current_path()?;
    let mut cfg = Config::load(&path)?;

    let info = network::discover();

    // Build a map of discovered values
    let mut discovered = std::collections::BTreeMap::new();

    if let Some(ip) = info.ip {
        discovered.insert("ip".to_string(), ip);
    }
    if let Some(gw) = info.gateway {
        discovered.insert("gateway".to_string(), gw);
    }
    if let Some(dns) = info.dns {
        discovered.insert("dc".to_string(), dns);
    }
    if let Some(dc_host) = info.dc_host {
        discovered.insert("dc_host".to_string(), dc_host);
    }
    if let Some(domain) = info.domain {
        discovered.insert("domain".to_string(), domain);
    }

    if json {
        // JSON output (implies dry-run)
        println!("{}", serde_json::to_string(&discovered)?);
        return Ok(());
    }

    // Normal text output
    if discovered.contains_key("ip") {
        println!("ip       = {}", discovered["ip"]);
    } else {
        eprintln!("penv: could not determine local IP address");
    }

    if discovered.contains_key("gateway") {
        println!("gateway  = {}", discovered["gateway"]);
    } else {
        eprintln!("penv: could not determine default gateway");
    }

    if discovered.contains_key("dc") {
        println!("dc       = {}", discovered["dc"]);
    } else {
        eprintln!("penv: could not determine DNS/DC server");
    }

    if discovered.contains_key("dc_host") {
        println!("dc_host  = {}", discovered["dc_host"]);
    }

    if discovered.contains_key("domain") {
        println!("domain   = {}", discovered["domain"]);
    } else {
        eprintln!("penv: could not determine domain name");
    }

    // Insert into config for saving
    for (k, v) in &discovered {
        cfg.vars.insert(k.clone(), v.clone());
    }

    if dry_run {
        println!("\n(dry run – not saved)");
    } else {
        cfg.save(&path)?;
        println!("\nSaved to {}", path.display());
    }
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
// penv print
// ---------------------------------------------------------------------------

fn cmd_print(profile_name: Option<&str>) -> anyhow::Result<()> {
    let (path, exact_profile_name) = match profile_name {
        Some(name) => {
            // Validate profile name and get exact name from stored profiles list
            validate_profile_name(name)?;
            let exact_name = find_exact_profile_name(name)?;
            let path = Config::profile_path(&exact_name)?;
            (path, Some(exact_name))
        },
        None => (Config::current_path()?, None),
    };

    let cfg = Config::load(&path)?;
    if cfg.vars.is_empty() {
        if let Some(exact_name) = exact_profile_name {
            println!("Profile '{}' has no variables set.", exact_name);
        } else {
            println!("No variables set. Run `penv discover` or `penv set <key> <value>`.");
        }
        return Ok(());
    }
    let max_key_len = cfg.vars.keys().map(|k| k.len()).max().unwrap_or(0);
    for (key, value) in &cfg.vars {
        println!("{key:<max_key_len$} = {value}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// penv list
// ---------------------------------------------------------------------------

fn cmd_list() -> anyhow::Result<()> {
    let profiles = get_stored_profiles()?;
    if profiles.is_empty() {
        println!("No saved profiles found. Use `penv store <name>` to save the current configuration.");
        return Ok(());
    }
    
    println!("Saved profiles:");
    for profile in profiles {
        println!("  {}", profile);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// penv clean
// ---------------------------------------------------------------------------

fn cmd_clean() -> anyhow::Result<()> {
    let path = Config::current_path()?;
    if path.exists() {
        fs::remove_file(&path)?;
    }
    println!("Cleared current.yaml");
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
    let exact_name = find_exact_profile_name(profile_name)?;
    let src = Config::profile_path(&exact_name)?;
    if !src.exists() {
        anyhow::bail!("Profile '{exact_name}' not found at {}", src.display());
    }
    let dst = Config::current_path()?;
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&src, &dst)?;
    println!("Profile '{exact_name}' loaded.");
    Ok(())
}

// ---------------------------------------------------------------------------
// penv drop
// ---------------------------------------------------------------------------

fn cmd_drop(profile_name: &str) -> anyhow::Result<()> {
    validate_profile_name(profile_name)?;
    
    // Get exact profile name from stored profiles list (security: never trust user input)
    let exact_name = find_exact_profile_name(profile_name)?;
    
    let path = Config::profile_path(&exact_name)?;
    if path.exists() {
        fs::remove_file(&path)?;
        println!("Profile '{exact_name}' deleted.");
    } else {
        anyhow::bail!("Profile '{exact_name}' file does not exist at {}", path.display());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// penv shell-init
// ---------------------------------------------------------------------------

fn cmd_shell_init(shell: Option<clap_complete::Shell>, autocomplete: bool) -> anyhow::Result<()> {
    use clap_complete::Shell;

    let shell = shell.unwrap_or_else(detect_shell);

    match shell {
        Shell::Bash => {
            print!("{}", SHELL_INIT_BASH);
            if autocomplete {
                print!("{}", AUTOCOMPLETE_BASH);
            }
        }
        Shell::Zsh => {
            print!("{}", SHELL_INIT_ZSH);
            if autocomplete {
                print!("{}", AUTOCOMPLETE_ZSH);
            }
        }
        Shell::Fish => {
            print!("{}", SHELL_INIT_FISH);
            if autocomplete {
                print!("{}", AUTOCOMPLETE_FISH);
            }
        }
        _ => anyhow::bail!("Unsupported shell. Use bash, zsh, or fish."),
    }

    Ok(())
}

fn cmd_list_profiles() -> anyhow::Result<()> {
    for p in get_stored_profiles()? {
        println!("{p}");
    }
    Ok(())
}

fn cmd_list_vars() -> anyhow::Result<()> {
    let path = Config::current_path()?;
    let cfg = Config::load(&path)?;
    for key in cfg.vars.keys() {
        println!("{key}");
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
            set|unset|load|discover|clean)
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
            set|unset|load|discover|clean)
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
            case set unset load discover clean
                eval (command penv init | string replace -a 'export ' 'set -gx ' | string replace -a '=' ' ')
        end
    end
    return $ret
end

# Initial load
eval (command penv init | string replace -a 'export ' 'set -gx ' | string replace -a '=' ' ')
"#;

const AUTOCOMPLETE_BASH: &str = r#"
# penv tab completion
_penv_complete() {
    local cur prev
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    local commands="init shell-init discover set unset print list clean store load drop completions"
    case "$prev" in
        penv)
            COMPREPLY=( $(compgen -W "$commands" -- "$cur") )
            ;;
        load|drop|print|store)
            COMPREPLY=( $(compgen -W "$(command penv _list-profiles 2>/dev/null)" -- "$cur") )
            ;;
        set|unset)
            COMPREPLY=( $(compgen -W "$(command penv _list-vars 2>/dev/null)" -- "$cur") )
            ;;
        completions|shell-init)
            COMPREPLY=( $(compgen -W "bash zsh fish elvish powershell" -- "$cur") )
            ;;
    esac
}
complete -F _penv_complete penv
"#;

const AUTOCOMPLETE_ZSH: &str = r#"
# penv tab completion
_penv_complete() {
    local state
    _arguments \
        '1: :->cmd' \
        '*: :->arg'
    case $state in
        cmd)
            local -a cmds
            cmds=(
                'init:Output export commands for eval'
                'shell-init:Output shell wrapper with auto-reload'
                'discover:Auto-detect network info'
                'set:Add or update a variable'
                'unset:Remove a variable'
                'print:Print active variables or a profile'
                'list:List all saved profiles'
                'clean:Wipe current.yaml'
                'store:Save current state as a profile'
                'load:Load a profile'
                'drop:Delete a saved profile'
                'completions:Generate shell completions'
            )
            _describe 'command' cmds
            ;;
        arg)
            case ${words[2]} in
                load|drop|print|store)
                    local -a profiles
                    profiles=(${(f)"$(command penv _list-profiles 2>/dev/null)"})
                    _describe 'profile' profiles
                    ;;
                set|unset)
                    local -a vars
                    vars=(${(f)"$(command penv _list-vars 2>/dev/null)"})
                    _describe 'variable' vars
                    ;;
                completions|shell-init)
                    _values 'shell' bash zsh fish elvish powershell
                    ;;
            esac
            ;;
    esac
}
compdef _penv_complete penv
"#;

const AUTOCOMPLETE_FISH: &str = r#"
# penv tab completion
set -l __penv_subcmds init shell-init discover set unset print list clean store load drop completions
complete -c penv -f
complete -c penv -n "not __fish_seen_subcommand_from $__penv_subcmds" -a init       -d 'Output export commands for eval'
complete -c penv -n "not __fish_seen_subcommand_from $__penv_subcmds" -a shell-init -d 'Output shell wrapper with auto-reload'
complete -c penv -n "not __fish_seen_subcommand_from $__penv_subcmds" -a discover   -d 'Auto-detect network info'
complete -c penv -n "not __fish_seen_subcommand_from $__penv_subcmds" -a set        -d 'Add or update a variable'
complete -c penv -n "not __fish_seen_subcommand_from $__penv_subcmds" -a unset      -d 'Remove a variable'
complete -c penv -n "not __fish_seen_subcommand_from $__penv_subcmds" -a print      -d 'Print active variables or a profile'
complete -c penv -n "not __fish_seen_subcommand_from $__penv_subcmds" -a list       -d 'List all saved profiles'
complete -c penv -n "not __fish_seen_subcommand_from $__penv_subcmds" -a clean      -d 'Wipe current.yaml'
complete -c penv -n "not __fish_seen_subcommand_from $__penv_subcmds" -a store      -d 'Save current state as a profile'
complete -c penv -n "not __fish_seen_subcommand_from $__penv_subcmds" -a load       -d 'Load a profile'
complete -c penv -n "not __fish_seen_subcommand_from $__penv_subcmds" -a drop       -d 'Delete a saved profile'
complete -c penv -n "not __fish_seen_subcommand_from $__penv_subcmds" -a completions -d 'Generate shell completions'
complete -c penv -n '__fish_seen_subcommand_from load drop print store' -a '(command penv _list-profiles 2>/dev/null)'
complete -c penv -n '__fish_seen_subcommand_from set unset'             -a '(command penv _list-vars 2>/dev/null)'
complete -c penv -n '__fish_seen_subcommand_from completions shell-init' -a 'bash zsh fish elvish powershell'
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

/// Find exact profile name from stored profiles list, matching case-insensitive.
/// Returns the exact name as stored in the filesystem to prevent trusting user input.
fn find_exact_profile_name(user_input: &str) -> anyhow::Result<String> {
    let stored_profiles = get_stored_profiles()?;
    
    // Find exact match (case-sensitive)
    if stored_profiles.contains(&user_input.to_string()) {
        return Ok(user_input.to_string());
    }
    
    // Find case-insensitive match
    let user_lower = user_input.to_lowercase();
    for profile in &stored_profiles {
        if profile.to_lowercase() == user_lower {
            return Ok(profile.clone());
        }
    }
    
    // No match found
    if stored_profiles.is_empty() {
        anyhow::bail!("No profiles saved yet. Use `penv store <name>` to save a profile.");
    } else {
        anyhow::bail!("Profile not found. Available profiles: {}", stored_profiles.join(", "));
    }
}

/// Get a list of all stored profile names (excluding current.yaml).
fn get_stored_profiles() -> anyhow::Result<Vec<String>> {
    let home = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
    let penv_dir = home.join(".local").join("share").join("penv");
    
    if !penv_dir.exists() {
        return Ok(vec![]);
    }
    
    let mut profiles = Vec::new();
    let entries = fs::read_dir(&penv_dir)?;
    
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                // Include all .yaml files except current.yaml
                if name.ends_with(".yaml") && name != "current.yaml" {
                    let profile_name = name.strip_suffix(".yaml").unwrap().to_string();
                    profiles.push(profile_name);
                }
            }
        }
    }
    
    profiles.sort();
    Ok(profiles)
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
