# Shell Completion for cossh

![shell_completion_example](../.resources/shell_example.png)


## Credits

The completion scripts in this directory are based on:
- [zsh-ssh](https://github.com/sunlei/zsh-ssh) by Sunlei - Thank you for the awesome scripts!


## Installation

### Fish Shell

#### Prerequisites
- [fzf](https://github.com/junegunn/fzf)
- fish or zsh
#### Installation Steps
   ```bash
   # Create directories if they don't exist
   mkdir -p ~/.config/fish/completions
   mkdir -p ~/.config/fish/functions
   
   # Copy the completion files
   cp fish/completions/cossh.fish ~/.config/fish/completions/
   cp fish/functions/__cossh_fzf_complete.fish ~/.config/fish/functions/
   ```

### Zsh Shell

#### Installation Steps
   ```bash
   # Create directory if it doesn't exist
   mkdir -p ~/.zsh/zsh-cossh
   
   # Copy the completion script
   cp zsh/zsh-cossh.zsh ~/.zsh/zsh-cossh/

   # Source the completion script in ~/.zshrc
   source ~/.zsh/zsh-cossh/zsh-cossh.zsh 
   ```

#### Adding Custom Descriptions

You can add descriptions to your SSH hosts by adding `#_desc` in your `~/.ssh/config`:

```ssh-config
Host myserver
    HostName example.com
    User myuser
    #_desc Production web server
```
## Uninstall
```bash
# Fish
rm -f ~/.config/fish/completions/cossh.fish
rm -f ~/.config/fish/functions/__cossh_fzf_complete.fish

# Zsh
# Remove the sourcing line from ~/.zshrc that references zsh-cossh.zsh
rm -f ~/.zsh/zsh-cossh/zsh-cossh.zsh
```