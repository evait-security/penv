# penv

Pentester Environment - A CLI tool for managing network and customer-specific environment variables across shell sessions on Linux.

## Overview

`penv` auto-discovers network configuration and exposes it as shell variables. It supports saving/loading profiles for different customer environments, making it easy to switch contexts during engagements.

Variables are stored in `~/.local/penv/current.yaml` and exported as lowercase shell variables (e.g., `$ip`, `$dc`, `$domain`) for direct use in commands like:

```bash
nxc smb $dc -d $domain -u $user -p $password
```

## Installation

```bash
cargo build --release
cp target/release/penv ~/.local/bin/
```

## Shell Setup

Add to your `.bashrc` or `.zshrc`:

```bash
eval "$(penv shell-init)"
```

This installs a wrapper function that:
- Exports all variables from `current.yaml` on shell startup
- Auto-reloads variables after `penv set`, `penv unset`, `penv load`, or `penv discover`

For fish shell:

```fish
eval (penv shell-init fish)
```

**Manual mode**: If you prefer explicit control, use `eval "$(penv init)"` instead. You'll need to re-run it after each change.

## Usage

### Auto-discover network environment

```bash
penv discover
```

Detects and saves:
- `ip` - Local IP of the primary LAN adapter
- `gateway` - Default gateway
- `dc` - DNS server (useful as DC in AD environments)
- `domain` - DNS search domain

### Manage variables

```bash
penv set user "USERNAME"
penv set password "P@ssw0rd!"
penv unset password
penv list
```

### Profile management

Save the current configuration as a named profile:

```bash
penv store customer_1
```

Load a saved profile:

```bash
penv load customer_1
```

With `shell-init`, variables are reloaded automatically.

Profiles are stored as `~/.local/penv/<name>.yaml`.

### Shell completions

Generate completions for your shell:

```bash
# Bash
penv completions bash > ~/.local/share/bash-completion/completions/penv

# Zsh
penv completions zsh > ~/.zfunc/_penv

# Fish
penv completions fish > ~/.config/fish/completions/penv.fish
```

## Commands

| Command | Description |
|---------|-------------|
| `penv init` | Output export commands for eval |
| `penv shell-init [shell]` | Output shell wrapper with auto-reload (bash/zsh/fish) |
| `penv discover` | Auto-detect network info and save to current.yaml |
| `penv set <key> <value>` | Add or update a variable |
| `penv unset <key>` | Remove a variable |
| `penv list` | Print all active variables |
| `penv clean` | Wipe current.yaml |
| `penv store <name>` | Save current state as a profile |
| `penv load <name>` | Load a profile into current.yaml |
| `penv completions <shell>` | Generate shell completions |

## Configuration Files

- `~/.local/penv/current.yaml` - Active configuration
- `~/.local/penv/<profile>.yaml` - Saved profiles

Example YAML:

```yaml
vars:
  ip: 192.168.1.50
  gateway: 192.168.1.1
  dc: 192.168.1.10
  domain: corp.local
  user: administrator
```

## Network Discovery

Discovery uses standard Linux tools with automatic fallbacks:

**IP and Gateway:**
- `ip route` - Default route interface and gateway
- `ip addr` - IP address of the default route interface

**DNS Server and Domain (tried in order):**
1. `resolvectl` - systemd-resolved (only if active)
2. `nmcli` - NetworkManager
3. `/run/systemd/resolve/resolv.conf` - upstream DNS when using systemd stub
4. `/etc/resolv.conf` - classic fallback

Works on systems with systemd-resolved, NetworkManager, or plain resolv.conf.

## License

MIT - see [LICENSE](LICENSE)