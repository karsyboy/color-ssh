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

Color SSH, `csh` for short, is a Rust-based wrapper for `ssh` that provides syntax-highlighted output using configurable rules. This tool enhances the usability of SSH by allowing regex-based syntax highlighting for specific text patterns in the output.

## Features

- **Regex-based Syntax Highlighting**: Match and highlight specific patterns in SSH output using configurable regex rules.
- **Customizable Colors**: Define colors for each rule using RGB hex codes.
- **Configurable Rules**: Use a YAML configuration file to add and manage highlighting rules dynamically.
- **Multi-line Regex Support**: Supports free-spacing regex for complex matching needs.
- **Simple Integration**: Works seamlessly with `ssh` as a drop-in replacement.

---

## Installation

### [Install using Latest Github Release](https://github.com/karsyboy/color-ssh/releases/)

### Install using Cargo

```bash
cargo install color-ssh
```

### Install using Github Source (Testing & Development)
1. Clone the repository:
   ```bash
   git clone https://github.com/karsyboy/color-ssh.git
   cd color-ssh
   ```

2. Build the project:
   ```bash
   cargo build --release
   ```

3. Install the binary (optional):
   ```bash
   cp target/release/csh /usr/local/bin/
   ```

---

## Usage

### Basic Syntax
```bash
A Rust-based SSH client with syntax highlighting.

Usage: csh [OPTIONS] <ssh_args>...

Arguments:
  <ssh_args>...  SSH arguments

Options:
  -d, --debug    Enable debug mode
  -L, --log      Enable SSH logging
  -h, --help     Print help
  -V, --version  Print version
```

### Example
```bash
csh user@hostname
```

This runs an SSH session while applying the syntax highlighting rules defined in `.csh-config.yaml`.

---

## Configuration
The configuration file is expected to be stored either in the users home directory under `.csh\.csh-config.yaml` or in the current directory the `csh`  binary tool is being ran out of.

Valid Configuration file locations:
- `$HOME/.csh/.csh-config.yaml`
- `$PWD/.csh-config.yaml`

The syntax highlighting rules are defined in a YAML file. Each rule consists of:
- **`description`**: A human-readable explanation of what the rule does.
- **`regex`**: The pattern to match in SSH output.
- **`color`**: The color to apply, specified as a key from the `palette` section.


If no configuration file is found then `csh` will create the configuration file `~/.csh/.csh-config.yaml` using the template `default.csh-config.yaml`.

### Example Configuration (`.\templates\default.csh-config.yaml`)
```yaml
# Description: This is the default template created by color-ssh (csh). 
# It contains information on the template layout and how to create a custom template.
# color-ssh templates can be found at https://github.com/karsyboy/color-ssh

# The palette section is used to define the colors that can be used in the rules section.
# The colors are defined in hex format.
palette:
  Red: '#c71800'
  Green: '#28c501'
  Blue: '#5698c8'

rules:
# example rule with all possible options
# - description: Match on the word "example"
#   regex: |
#     (?ix)
#     \b
#     example
#     \b
#   color: Kelly-Green
# create a rule that matches on the word "connected" or "up" and color it Kelly-Green

# Example of a rule that uses a one line regex to match on "good" or "up" and color it Green
- description: Match on good keywords
  regex: (?ix)\b(good|up)\b
  color: Green


- description: Match on neutral keywords
  regex: |
    (?ix)
    \b
    neutral
    \b
  color: Blue

# create a rule that matches on the word "down" or "error" or "disabled" and color it Red
- description: Match on bad keywords
  regex: |
    (?ix)
    \b
    (down|error|disabled)
    \b
  color: Red
```

### Explanation of Configuration
1. **Palette**:
   - The `palette` section defines reusable colors using hex codes (e.g., `#32cd32` for green).
2. **Rules**:
   - Each rule includes a description, regex, and a reference to a color in the `palette`.

## Logging 
The `.csh` folder is also used to stored ssh logs if the `-L or --log` argument is used. Logs will be stored in `.csh/ssh-logs/MM-DD-YYYY/HOSTNAME-MM-DD-YYYY.log`

---

## How It Works

1. **Regex Matching**:
   - The script processes SSH output chunk by chunk, applying each regex rule to match patterns.
2. **ANSI Escape Codes**:
   - Matches are wrapped with ANSI escape codes for colors based on the palette.
3. **Dynamic Configuration**:
   - Rules and colors can be updated by modifying the YAML config file without changing the code.

---

## Licensing

This project is licensed under the MIT License. See the full license below:

### MIT License
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

## Contributing

Contributions are welcome! To contribute:
1. Fork the repository.
2. Create a feature branch.
3. Commit your changes with detailed messages.
4. Open a pull request.

---

## Future Improvements

- Add vault functionality 
  - `csh-vault.yaml` encrypted file for storing host information like passwords used to connect.
  - File with be encrypted with quantum computer resistance encryption methods.
  - Feature to unlock csh which spawns a csh process that has the ability to decrypt the `csh-vault.yaml` after the users provides there unlock password.
  - and many more features around this...
- Add support for themes or predefined configurations.

---