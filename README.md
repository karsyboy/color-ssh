<p align="center">
  <img src="https://raw.githubusercontent.com/karsyboy/color-ssh/refs/heads/main/.resources/title.svg" />
</p>

<p align="center">
    <a href="https://github.com/karsyboy/color-ssh/releases">
        <img src="https://img.shields.io/github/v/release/karsyboy/color-ssh?include_prereleases&logo=GitHub&label=Github"></a>
    <a href="https://crates.io/crates/color-ssh">
        <img src="https://img.shields.io/crates/v/color-ssh?logo=Rust"></a>
  <br>
    <a href="https://github.com/karsyboy/color-ssh/actions/workflows/release-plz.yml">
        <img src="https://img.shields.io/github/actions/workflow/status/karsyboy/color-ssh/release-plz.yml?logo=githubactions&logoColor=white&label=CI%2FCD"></a>
    <a href="https://github.com/karsyboy/color-ssh/actions/workflows/release.yml">
        <img src="https://img.shields.io/github/actions/workflow/status/karsyboy/color-ssh/release.yml?logo=Rust&label=Release%20Build"></a>
<p>

---

## About

**Color SSH** (`colorsh`) is a powerful Rust-based wrapper for SSH that enhances your terminal experience with real-time syntax highlighting and intelligent logging. Built for network engineers, system administrators, and anyone who works extensively with SSH, `colorsh` transforms plain SSH output into beautifully highlighted text using customizable, regex-based rules.

Whether you're managing network devices, debugging servers, or analyzing logs, Color SSH makes it easier to spot critical information at a glance. Errors will stand out in red, successful operations will glow green, and everything is configurable to match your workflow.

---

## Features

- **🎨 Real-time Syntax Highlighting**: Apply regex-based color rules to SSH output as it streams
- **⚙️ Highly Configurable**: YAML-based configuration with custom color palettes and regex rules
- **📝 Session Logging**: Automatic logging of SSH sessions with organized date-based storage
- **🔒 Secret Redaction**: Automatically remove sensitive data (passwords, keys, hashes) from logs
- **📋 Profile Support**: Multiple configuration profiles for different environments (network devices, servers, etc.)
- **🎯 Template Library**: Pre-built templates for network equipment and common use cases - community contributions welcome!
- **🔄 Hot Reload**: Configuration changes apply automatically without restarting
- **🐚 Shell Integration**: Enhanced tab completion and interactive host selection for Fish and Zsh
- **🚀 Drop-in Replacement**: Works seamlessly as an SSH wrapper - just use `colorsh` instead of `ssh`

---

## Installation

### Using Pre-built Binaries (Recommended)

Download the latest release from [GitHub Releases](https://github.com/karsyboy/color-ssh/releases/) for your platform.

### Using Cargo

If you have Rust installed, install directly from crates.io:

```bash
cargo install color-ssh
```

### From Source

For development or testing the latest changes:

```bash
# Clone the repository
git clone https://github.com/karsyboy/color-ssh.git
cd color-ssh

# Build the release binary
cargo build --release

# Optional: Install to system path
sudo cp target/release/colorsh /usr/local/bin/
```

### Verify Installation

```bash
colorsh --version
```

---

## Uninstall

### Using Cargo

If you installed using cargo:

```bash
cargo uninstall color-ssh
```

Note: This only removes the binary. To completely remove configuration files and logs, also run:

```bash
rm -rf ~/.colorsh/
```

### Using Homebrew

If you installed using Homebrew:

```bash
brew uninstall color-ssh
```

### Linux/macOS (Manual)

If you installed using the installer script, follow these steps:

```bash
# 1. Remove the main binary
rm ~/.cargo/bin/colorsh

# 2. Remove the updater binary (if installed)
rm -f ~/.cargo/bin/color-ssh-update

# 3. Remove configuration and logs
rm -rf ~/.colorsh/

# 4. Remove the installation receipt
rm -rf ~/.config/color-ssh/
```

### Windows

If you installed using the PowerShell installer script:

```powershell
# 1. Remove the main binary
Remove-Item "$env:USERPROFILE\.cargo\bin\colorsh.exe" -Force

# 2. Remove the updater binary (if installed)
Remove-Item "$env:USERPROFILE\.cargo\bin\color-ssh-update.exe" -Force -ErrorAction SilentlyContinue

# 3. Remove configuration and logs
Remove-Item "$env:USERPROFILE\.colorsh" -Recurse -Force -ErrorAction SilentlyContinue

# 4. Remove the installation receipt
if ($env:XDG_CONFIG_HOME) {
    Remove-Item "$env:XDG_CONFIG_HOME\color-ssh" -Recurse -Force -ErrorAction SilentlyContinue
} else {
    Remove-Item "$env:LOCALAPPDATA\color-ssh" -Recurse -Force -ErrorAction SilentlyContinue
}
```

### Shell Completion Cleanup

If you installed shell completions, remove them as well:

```bash
# Fish
rm -f ~/.config/fish/completions/colorsh.fish
rm -f ~/.config/fish/functions/__colorsh_fzf_complete.fish

# Zsh
# Remove the sourcing line from ~/.zshrc that references zsh-colorsh.zsh
```

---

## Usage

### Basic Command Structure

```bash
colorsh [OPTIONS] <ssh_args>...
```

### Options

| Option | Description |
|--------|-------------|
| `-d, --debug` | Enable debug mode with detailed logging to `~/.colorsh/logs/colorsh.log` |
| `-l, --log` | Enable SSH session logging to `~/.colorsh/logs/ssh_sessions/` |
| `-P, --profile <name>` | Use a specific configuration profile |
| `-h, --help` | Display help information |
| `-V, --version` | Display version information |

### Examples

```bash
# Basic SSH connection with syntax highlighting
colorsh user@hostname

# Enable session logging
colorsh -l admin@router.example.com

# Use a specific configuration profile
colorsh -P network cisco@switch.local

# Debug mode for troubleshooting
colorsh -d user@server.com

# Combine options (logging + profile)
colorsh -l -P network user@firewall.example.com

# Pass SSH arguments through
colorsh -l user@host -p 2222 -i ~/.ssh/custom_key

# Non-interactive SSH commands (highlighting disabled automatically)
colorsh user@host -G          # Dump SSH configuration
colorsh user@host -T          # Disable pseudo-terminal
```

### Session Logs

When using the `-l` or `--log` flag, SSH sessions are logged to:

```
~/.colorsh/logs/ssh_sessions/YYYY-MM-DD/HOSTNAME.log
```

Example:
```
~/.colorsh/logs/ssh_sessions/2026-01-26/router1.log
```

---

## Configuration

### Configuration File Locations

Color SSH looks for configuration files in the following order:

1. **Current directory**: `./[profile].colorsh-config.yaml`
2. **Home directory**: `~/.colorsh/[profile].colorsh-config.yaml`

If no configuration file exists, Color SSH will automatically create a default configuration at `~/.colorsh/.colorsh-config.yaml`.

### Configuration Profiles

Use profiles to maintain different configurations for different environments:

```bash
# Default profile
~/.colorsh/.colorsh-config.yaml

# Network devices profile
~/.colorsh/network.colorsh-config.yaml

# Usage
colorsh -P network user@switch.local
```

### Configuration Structure

A configuration file consists of three main sections:

#### 1. Settings

Optional settings for controlling Color SSH behavior:

```yaml
settings:
  show_title: true              # Display a colored title banner
  debug_mode: false             # Enable debug logging
  ssh_logging: true             # Enable session logging by default
  remove_secrets:               # Regex patterns to redact from logs
    - '9[\s]\$9\$.*'           # Juniper type 9 secrets
    - 'sha512[\s]\$6\$.*'      # SHA-512 hashes
    - 'ssh-ed25519[\s].*'      # SSH public keys
```

#### 2. Color Palette

Define reusable colors using hex codes:

```yaml
palette:
  Red: '#c71800'
  Green: '#28c501'
  Blue: '#5698c8'
  Orange: '#e67547'
  Gold: '#a35a00'
```

#### 3. Highlighting Rules

Define regex patterns and their associated colors:

```yaml
rules:
  - description: Highlight successful operations
    regex: (?ix)\b(success|ok|connected|up|enabled)\b
    color: Green

  - description: Highlight errors and failures
    regex: (?ix)\b(error|fail|down|disabled|denied)\b
    color: Red

  - description: Highlight IP addresses
    regex: \b(?:\d{1,3}\.){3}\d{1,3}\b
    color: Blue
```

### Example: Default Configuration

The default configuration template (`templates/default.colorsh-config.yaml`) provides basic keyword highlighting:

```yaml
palette:
  Red: '#c71800'
  Green: '#28c501'
  Blue: '#5698c8'

rules:
  - description: Match on good keywords
    regex: (?ix)\b(good|up|success|ok|connected)\b
    color: Green

  - description: Match on neutral keywords
    regex: (?ix)\b(neutral|info|status)\b
    color: Blue

  - description: Match on bad keywords
    regex: (?ix)\b(down|error|disabled|fail|denied)\b
    color: Red
```

### Example: Network Devices Configuration

For network engineers, the `templates/network.colorsh-config.yaml` template provides extensive highlighting for Cisco and other network devices (Reference the actual file for a detailed config):

```yaml
settings:
  remove_secrets:
    - '9[\s]\$9\$.*'           # Juniper secrets
    - 'sha512[\s]\$6\$.*'      # Password hashes
    - '7[\s][0-9]{2}[0-9A-Fa-f]+$'  # Cisco type 7
  show_title: true
  ssh_logging: true

palette:
  Orange: '#e67547'
  Aqua: '#00e0d1'
  Gold: '#a35a00'
  Green: '#28c501'
  Red: '#c71800'

rules:
  - description: Cisco enable mode prompt
    regex: (\S+)#
    color: Orange

  - description: Cisco user mode prompt
    regex: (\S+)>
    color: Gold

  - description: Interface names
    regex: (?ix)\b(GigabitEthernet|FastEthernet|Vlan|Port-channel)\d+(/\d+)*(\.\d+)?\b
    color: Green
```

### Regex Tips

Color SSH uses Rust's `regex` crate with support for:

- **Case-insensitive matching**: Use `(?i)` flag
- **Extended mode** (ignore whitespace): Use `(?x)` flag
- **Multi-line patterns**: Use `|` for multi-line regex blocks in YAML
- **Word boundaries**: Use `\b` to match whole words
- **Groups**: Use `()` for capturing groups

Example of a well-structured rule:

```yaml
- description: Match Cisco interface types
  regex: |
    (?ix)                          # Case-insensitive, extended mode
    \b                             # Word boundary
    (gigabitethernet|gi|
     tengigabitethernet|te|
     fastethernet|fa)
    \d+(/\d+)*(\.\d+)?            # Port numbers
    \b                             # Word boundary
  color: Green
```

### Shell Completion

Color SSH includes advanced shell completion features for Fish and Zsh shells, including:

- Tab completion for SSH hosts from your `~/.ssh/config`
- Interactive host selection with fuzzy finding (fzf)
- Host descriptions and previews
- Support for SSH config `Include` directives

For detailed installation and usage instructions, see the [Shell Completion README](shell-completion/README.md).

---

## Contributing

Contributions are welcomed! Here's how to get started:

### 1. Fork and Clone

```bash
git clone https://github.com/YOUR-USERNAME/color-ssh.git
cd color-ssh
```

### 2. Create a Feature Branch

```bash
git checkout -b feature/your-feature-name
```

### 3. Make Your Changes

- Write clean, idiomatic Rust code
- Add tests for new functionality
- Update documentation as needed
- Follow existing code style and conventions

### 4. Test Your Changes

```bash
# Run tests
cargo test

# Build and test locally
cargo build --release
./target/release/colorsh user@testhost

# Run linter
cargo clippy

# Format code
cargo fmt
```

### 5. Commit and Push

```bash
git add .
git commit -m "feat: add your feature description"
git push origin feature/your-feature-name
```

### 6. Open a Pull Request

- Provide a clear description of your changes
- Reference any related issues
- Ensure CI/CD checks pass

### Contribution Ideas

- 🎨 New configuration templates for specific platforms
- 🐛 Bug fixes and performance improvements
- 📚 Documentation enhancements
- ✨ New features — check the roadmap or propose your own ideas
- 🧪 Additional test coverage

### Code of Conduct

Please be respectful and constructive. We're building this together!

---

## License

This project is licensed under the **MIT License**.

```
MIT License

Copyright (c) 2025 Karsyboy

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

---

## Roadmap

- 🔄 **Coming Soon**

---

<p align="center">Made with ❤️ by <a href="https://github.com/karsyboy">@karsyboy</a></p>