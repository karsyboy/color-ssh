# Shell Completion for cossh

## Installation

### Fish Shell
#### Installation Steps
   ```bash
   # Create directories if they don't exist
   mkdir -p ~/.config/fish/completions

   # Copy the completion file
   cp fish/completions/cossh.fish ~/.config/fish/completions/
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

## Uninstall
```bash
# Fish
rm -f ~/.config/fish/completions/cossh.fish

# Zsh
# Remove the sourcing line from ~/.zshrc that references zsh-cossh.zsh
rm -f ~/.zsh/zsh-cossh/zsh-cossh.zsh
```
