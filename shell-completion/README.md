# Shell Completion for colorsh

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
   cp fish/completions/colorsh.fish ~/.config/fish/completions/
   cp fish/functions/__colorsh_fzf_complete.fish ~/.config/fish/functions/
   ```

### Zsh Shell

#### Installation Steps
   ```bash
   # Create directory if it doesn't exist
   mkdir -p ~/.zsh/zsh-colorsh
   
   # Copy the completion script
   cp zsh/zsh-colorsh.zsh ~/.zsh/zsh-colorsh/

   # Source the completion script in ~/.zshrc
   source ~/.zsh/zsh-colorsh/zsh-colorsh.zsh 
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
rm -f ~/.config/fish/completions/colorsh.fish
rm -f ~/.config/fish/functions/__colorsh_fzf_complete.fish

# Zsh
# Remove the sourcing line from ~/.zshrc that references zsh-colorsh.zsh
rm -f ~/.zsh/zsh-colorsh/zsh-colorsh.zsh
```

## Auto Login with sshpass using GPG Encryption

This section describes how to use `sshpass` with GPG-encrypted passwords for automated SSH login.

### Setup

1. **Encrypt your password** with GPG:
   ```bash
   nano .sshpasswd
   gpg -c .sshpasswd
   rm .sshpasswd
   ```

2. **Load the password** into an environment variable:
   ```bash
   source ssh-in.sh
   ```

3. **Connect using sshpass**:
   ```bash
   sshpass -e colorsh <hostname>
   ```

4. **Clear the password** from the environment when done:
   ```bash
   source ssh-out.sh
   ```

### Tab Completion for sshpass Aliases

If you create an alias (Ex. `colorshp`) for the sshpass command you can set up tab completion:

#### Fish

Create a new completion file for your alias:
```bash
cp fish/completions/colorsh.fish ~/.config/fish/completions/colorshp.fish
sed -i 's/colorsh/colorshp/g' ~/.config/fish/completions/colorshp.fish
```
Then add to your `~/.config/fish/config.fish`:
```bash
alias colorshp='sshpass -e colorsh'
```

#### Zsh

Create a new completion script for your alias:
```bash
cp zsh/zsh-colorsh.zsh ~/.zsh/zsh-colorshp/zsh-colorshp.zsh
sed -i 's/colorsh/colorshp/g' ~/.zsh/zsh-colorshp/zsh-colorshp.zsh
```

Then add to your `~/.zshrc`:
```bash
source ~/.zsh/zsh-colorshp/zsh-colorshp.zsh
alias colorshp='sshpass -e colorsh'
```