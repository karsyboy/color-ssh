# CSH (Colored SSH)

`csh` is a Rust-based wrapper for `ssh` that provides syntax-highlighted output using configurable rules. This tool enhances the usability of SSH by allowing regex-based syntax highlighting for specific text patterns in the output.

## Features

- **Regex-based Syntax Highlighting**: Match and highlight specific patterns in SSH output using configurable regex rules.
- **Customizable Colors**: Define colors for each rule using RGB hex codes.
- **Configurable Rules**: Use a YAML configuration file to add and manage highlighting rules dynamically.
- **Multi-line Regex Support**: Supports free-spacing regex for complex matching needs.
- **Simple Integration**: Works seamlessly with `ssh` as a drop-in replacement.

---

## Installation

1. Clone the repository:
   ```bash
   git clone https://github.com/karsyboy/csh.git
   cd csh
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
csh <ssh arguments>
```

### Example
```bash
csh user@hostname
```

This runs an SSH session while applying the syntax highlighting rules defined in `.csh-config.yaml`.

---

## Configuration
The ocnfiguration file is expected to be stored either in the users home directory as `.csh-config.yaml` or in the current directory the `csh` tool is being ran out of.

Valid Configuration file locations:
- `$HOME/.csh-config.yaml`
- `$PWD/.csh-config.yaml`

The syntax highlighting rules are defined in a YAML file. Each rule consists of:
- **`description`**: A human-readable explanation of what the rule does.
- **`regex`**: The pattern to match in SSH output.
- **`color`**: The color to apply, specified as a key from the `palette` section.

### Example Configuration (`.csh-config.yaml`)
```yaml
palette:
  Sunrise-Orange: '#e67549'
  Aqua-Blue: '#00e0d1'
  Hot-Pink: '#FF69B4'
  Celestial-Blue: '#5698c8'
  Rich-Gold: '#a35a00'
  Bright-Ube: '#df99f0'
  Caribbean-Green: '#03d28d'
  Milano-Red: '#c71800'
  Sedona: '#c96901'
  Ochre: '#ca9102'
  Mustard: '#cab902'
  Bright-Olive: '#a2bc02'
  Dark-Lime-Green: '#79bf02'
  Kelly-Green: '#28c501'

rules:
# Switch Prompt
- description: Prompt in enabled mode for network switches
  regex: ((?:\r\n)|^)+([a-zA-Z0-9_-]+#)
  color: Rich-Gold

- description: Match prompt in disable for mode network switches
  regex: ((?:\r\n)|^)+([a-zA-Z0-9_-]+>)
  color: Rich-Gold

# Interfaces
- description: Always color the word "interface"
  regex: |
    (?ix)
      \b(
      interfaces?|
      \w+-interface
      )\b
  color: Kelly-Green

- description: Match on interface type "Ethernet"
  regex: |
    (?ix)
      \b(
      (ethernet|eth|et)
      (\d{1,2})?
      (/\d{1,2})?
      (/\d{1,2})?
      (\.\d{1,4})?
      )\b
  color: Sedona

- description: Match on Cisco interface types
  regex: |
    (?ix)
      \b
      (gigabitethernet|gi|gig|
      twogigabitethernet|tw|
      tengigabitethernet|te|
      twentyfivegige|twe|
      fortygigabitethernet|fo|
      appgigabitethernet|ap)
      (\d{1,2})?
      (/\d{1,2})?
      (/\d{1,2})?
      (\.\d{1,4})?
      \b
  color: Sedona

- description: Match on type "Vlan"
  regex: |
    (?ix)
      \b
      (vlan|vl)
      (\d{1,4}|\s\d{1,4})?
      ((?:,\d{1,4})*)?
      \b
  color: Dark-Lime-Green

- description: Match on type "Port-Channel"
  regex: |
    (?ix)
      \b(
      (port-channel|po)
      (\d{1,4})?
      (\.\d{1,4})?
      )\b
  color: Bright-Olive

- description: Match on Extra interface types
  regex: |
    (?ix)
      \b
      (management|mgmt|
      loopback|lo|
      tunnel|tu)
      (\d{1,4})?
      \b
  color: Mustard

# Keywords
- description: Match on good keywords
  regex: |
    (?ix)
      \b
      (connected|up)
      \b
  color: Kelly-Green

- description: Match on nutral keywords
  regex: |
    (?ix)
      \b
      (xcvrAbsen|noOperMem|notconnect)
      \b
  color: Sunrise-Orange

- description: Match on bad keywords
  regex: |
    (?ix)
      \b
      (down|shutdown)
      \b
  color: Milano-Red

# URLs and IPs 
- description: URL
  regex: (?i)\b(((htt|ft|lda)ps?|telnet|ssh|tftp)://[^\s/$.?#].[^\s]*)\b
  color: Aqua-Blue

- description: IPv4
  regex: i?(?:[0-9]{1,3}\.){3}[0-9]{1,3}(?:\/[0-9]{1,2}|:[0-9]{1,5})?(?:,(?:[0-9]{1,5})?)?
  color: Celestial-Blue

- description: Subnet Mask
  regex: (?:)(?:0|255)\.(?:[0-9]{1,3}\.){2}[0-9]{1,3}
  color: Celestial-Blue

- description: IPv6
  regex: |
    (([0-9a-fA-F]{1,4}:){7,7}[0-9a-fA-F]{1,4}|
    ([0-9a-fA-F]{1,4}:){1,7}:|
    ([0-9a-fA-F]{1,4}:){1,6}:[0-9a-fA-F]{1,4}|
    ([0-9a-fA-F]{1,4}:){1,5}(:[0-9a-fA-F]{1,4}){1,2}|
    ([0-9a-fA-F]{1,4}:){1,4}(:[0-9a-fA-F]{1,4}){1,3}|
    ([0-9a-fA-F]{1,4}:){1,3}(:[0-9a-fA-F]{1,4}){1,4}|
    ([0-9a-fA-F]{1,4}:){1,2}(:[0-9a-fA-F]{1,4}){1,5}|
    [0-9a-fA-F]{1,4}:((:[0-9a-fA-F]{1,4}){1,6})|
    :((:[0-9a-fA-F]{1,4}){1,7}|:)|
    fe80:(:[0-9a-fA-F]{0,4}){0,4}%[0-9a-zA-Z]{1,}|
    ::(ffff(:0{1,4}){0,1}:){0,1}
    ((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}
    (25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])|
    ([0-9a-fA-F]{1,4}:){1,4}:
    ((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}
    (25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9]))
  color: Celestial-Blue
```

### Explanation of Configuration
1. **Palette**:
   - The `palette` section defines reusable colors using hex codes (e.g., `#32cd32` for green).
2. **Rules**:
   - Each rule includes a description, regex, and a reference to a color in the `palette`.

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

Copyright (c) 2025 [Your Name]

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

- Add support for themes or predefined configurations.
- Enhance error reporting for invalid regex patterns in the config file.
- Add the ability to have the tool output all text out to a log file

---

