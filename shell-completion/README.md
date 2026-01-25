# Shell Completion for CSH (Color SSH)

This directory contains shell completion integrations for `csh` (Color SSH) to provide enhanced tab completion and interactive host selection for both **Fish** and **Zsh** shells.

## Credits

The completion scripts in this directory are based on:
- **Zsh**: [zsh-ssh](https://github.com/sunlei/zsh-ssh) by Sunlei - Modified to work with the `csh` CLI utility
- **Fish**: Custom implementation based off [zsh-ssh](https://github.com/sunlei/zsh-ssh) by Sunlei

## Features

- **Smart SSH Config Parsing**: Automatically parses your `~/.ssh/config` file and handles `Include` directives
- **Tab Completion**: Quick access to your configured SSH hosts
- **Interactive Selection**: Uses fzf for a beautiful interactive host picker with live preview
- **Host Information Display**: Shows hostname, user, and custom descriptions from your SSH config

## Installation

### Fish Shell

#### Prerequisites
- [fzf](https://github.com/junegunn/fzf) (for interactive completion)

#### Installation Steps

1. **Copy completion files** to your Fish config directory:
   ```bash
   # Create directories if they don't exist
   mkdir -p ~/.config/fish/completions
   mkdir -p ~/.config/fish/functions
   
   # Copy the completion files
   cp fish/completions/csh.fish ~/.config/fish/completions/
   cp fish/functions/__csh_fzf_complete.fish ~/.config/fish/functions/
   ```

2. **Usage**:
   - Type `csh` and press `Tab` to open the interactive fzf selector
   - Use arrow keys or type to filter hosts
   - Press `Enter` to select and execute
   - Press `Alt-Enter` to select without executing
   - The preview pane shows the full SSH configuration for the selected host

#### Adding Custom Descriptions

You can add descriptions to your SSH hosts by adding `#_desc` comments in your `~/.ssh/config`:

```ssh-config
Host myserver
    HostName example.com
    User myuser
    #_desc Production web server
```

### Zsh Shell

#### Installation Steps

1. **Copy the completion script** to your Zsh config directory:
   ```bash
   # Create directory if it doesn't exist
   mkdir -p ~/.zsh/zsh-csh
   
   # Copy the completion script
   cp zsh/zsh-csh.zsh ~/.zsh/zsh-csh/
   ```

2. **Add to your `~/.zshrc`**:
   ```bash
   # Source the completion script
   source ~/.zsh/zsh-csh/zsh-csh.zsh
   
   # Enable completion for csh (uses same completion as ssh)
   compdef csh=ssh
   ```

3. **Reload your Zsh configuration**:
   ```bash
   source ~/.zshrc
   ```

4. **Usage**:
   - Type `csh` and press `Tab` to see available hosts
   - Continue typing to filter, or select from the list

## Troubleshooting

### Fish Completion Not Working
- Ensure fzf is installed: `fzf --version`
- Check that the files are in the correct locations
- Try `fish_update_completions` to refresh Fish's completion cache

### Zsh Completion Not Working
- Make sure the script is sourced in your `.zshrc`
- Verify that `compdef` is called after sourcing the script
- Try `compinit` to reinitialize completions

### No Hosts Appearing
- Verify your `~/.ssh/config` file exists and contains `Host` entries
- Make sure host entries don't use wildcards (`*`)
- Check file permissions on your SSH config