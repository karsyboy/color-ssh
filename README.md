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

**Color SSH** (`cossh`) is a Rust-based wrapper for SSH that enhances your terminal experience with real-time syntax highlighting and session logging. Built for network engineers, system administrators, and anyone who works with devices that have basic shells.

![cossh_example](./.resources/cossh_example.png)

## Features

- Session Manger TUI
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
Usage: cossh [OPTIONS] [ssh_args]...

Arguments:
  [ssh_args]...  SSH arguments to forward to the SSH command

Options:
  -d, --debug              Enable debug mode with detailed logging to ~/.color-ssh/logs/cossh.log
  -l, --log                Enable SSH session logging to ~/.color-ssh/logs/ssh_sessions/
  -P, --profile <profile>  Specify a configuration profile to use
  -t, --test               Ignore config logging settings; only use CLI -d/-l logging flags
  -h, --help               Print help
  -V, --version            Print version


cossh                                              # Launch interactive session manager
cossh -d                                           # Launch interactive session manager with debug enabled
cossh -d user@example.com                          # Debug mode enabled
cossh -l user@example.com                          # SSH logging enabled
cossh -l -P network user@firewall.example.com      # Use 'network' config profile
cossh -l user@host -p 2222                         # Both modes with SSH args
cossh -tld -P network localhost                    # Test mode: force logging from CLI flags only
cossh user@host -G                                 # Non-interactive command
```


## Configuration

#### Rule Config

Configuration files are looked for in the following order:

1. **Current directory**: `./[profile].cossh-config.yaml`
2. **Home directory**: `~/.color-ssh/[profile].cossh-config.yaml`

If no configuration file is found the default configuration will be created at `~/.color-ssh/cossh-config.yaml`.

#### Color-SSH TUI Metadata in `~/.ssh/config`

The interactive session manger supports metadata comments inside the SSH config file.

| Tag | What it does |
| --- | --- |
| `#_Desc <text>` | Adds description in the info view. |
| `#_Profile <name>` | Opens that host using the matching cossh profile (`[profile].cossh-config.yaml`). |
| `#_pass <name>` | Decrypts `~/.color-ssh/keys/<name>.gpg` and uses password auto-login for that host. |
| `#_hidden <true\|yes\|1>` | Hides the host from the interactive host list. |

```sshconfig
Host switch01
    HostName switch01.example.com
    User admin
    #_Profile network
    #_Desc Example Switch
    #_pass test_pass
```

For more info on the TUI go here [TUI User Guide](docs/TUI_USER_GUIDE.md).

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
rm ~/.cargo/bin/cossh

# 2. Remove the updater binary
rm -f ~/.cargo/bin/color-ssh-update

# 3. (Optional) Remove configuration and logs
rm -rf ~/.color-ssh/

# 4. Remove the installation receipt
rm -rf ~/.config/color-ssh/
```

### Shell Completion Cleanup
For instructions see the [Shell Completion README](shell-completion/README.md).

## Support
If you need help, have an issue, or just want to make a sugestion / request a feature please open an [issue](https://github.com/karsyboy/color-ssh/issues/new). 

## Special Thanks

Thanks to the following projects for the inspiration behind Color SSH.

- [Chromaterm](https://github.com/hSaria/ChromaTerm)
- [netcli-highlight](https://github.com/danielmacuare/netcli-highlight)

Also thank you to [Alacritty](https://github.com/alacritty/alacritty) for the terminal create that is being used to render the terminal in the TUI
