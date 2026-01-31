<p align="center">
  <img src="./.resources/title.svg" />
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

## About

**Color SSH** (`colorsh`) is a Rust-based wrapper for SSH that enhances your terminal experience with real-time syntax highlighting and session logging. Built for network engineers, system administrators, and anyone who works with devices that have basic shells.

![colorsh_example](./.resources/colorsh_example.png)

## Features

- Syntax highlighting
- Session logging
- Configuration hot reload
- Mutliple Profile Support
- Configurable rules using regex matching


## Installation

### Pre-built Binaries (Recommended)
Download the latest release from [GitHub Releases](https://github.com/karsyboy/color-ssh/releases/) for your platform.

### Cargo
```bash
cargo install color-ssh
```

### From Source
```bash
# Clone the repository
git clone https://github.com/karsyboy/color-ssh.git
cd color-ssh

# Build the release binary
cargo build --release
```

### Shell Completion
Shell completeion scripts are included for `fish` and `zsh`. For instructions see the [Shell Completion README](shell-completion/README.md).


## Usage

```bash
Usage: colorsh [OPTIONS] <ssh_args>...

Arguments:
  <ssh_args>...  SSH arguments to forward to the SSH command

Options:
  -d, --debug              Enable debug mode with detailed logging to ~/.colorsh/logs/colorsh.log
  -l, --log                Enable SSH session logging to ~/.colorsh/logs/ssh_sessions/
  -P, --profile <profile>  Specify a configuration profile to use [default: ]
  -h, --help               Print help
  -V, --version            Print version


colorsh -d user@example.com                          # Debug mode enabled
colorsh -l user@example.com                          # SSH logging enabled
colorsh -l -P network user@firewall.example.com      # Use 'network' config profile
colorsh -l user@host -p 2222 -i ~/.ssh/custom_key    # Both modes with SSH args
colorsh user@host -G                                 # Non-interactive command
```


## Configuration

Configuration files are looked for in the following order:

1. **Current directory**: `./[profile].colorsh-config.yaml`
2. **Home directory**: `~/.colorsh/[profile].colorsh-config.yaml`

If no configuration file is found the default configuration will be created at  `~/.colorsh/.colorsh-config.yaml`.


## Uninstall

### Cargo
```bash
cargo uninstall color-ssh
```

### Homebrew
```bash
brew uninstall color-ssh
```

### Linux/macOS (Manual)
```bash
# 1. Remove the main binary
rm ~/.cargo/bin/colorsh

# 2. Remove the updater binary
rm -f ~/.cargo/bin/color-ssh-update

# 3. (Optional) Remove configuration and logs
rm -rf ~/.colorsh/

# 4. Remove the installation receipt
rm -rf ~/.config/color-ssh/
```

### Windows
```powershell
# 1. Remove the main binary
Remove-Item "$env:USERPROFILE\.cargo\bin\colorsh.exe" -Force

# 2. Remove the updater binary
Remove-Item "$env:USERPROFILE\.cargo\bin\color-ssh-update.exe" -Force

# 3. (Optional) Remove configuration and logs
Remove-Item "$env:USERPROFILE\.colorsh" -Recurse -Force

# 4. Remove the installation receipt
Remove-Item "$env:LOCALAPPDATA\color-ssh" -Recurse -Force
```

### Shell Completion Cleanup
For instructions see the [Shell Completion README](shell-completion/README.md).

## Support
If you need help, have an issue, or just want to make a sugestion / request a feature please open an [issue](https://github.com/karsyboy/color-ssh/issues/new). 

## Special Thanks

Thanks to the following projects for the insperation behind Color SSH.

- [Chromaterm](https://github.com/hSaria/ChromaTerm)
- [netcli-highlight](https://github.com/danielmacuare/netcli-highlight)