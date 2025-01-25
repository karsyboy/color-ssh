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
   git clone <repository-url>
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
The ocnfiguration file is expected to be store either in the users home directory as `.csh-config.yaml` or in the current directory the `csh` tool is being ran out of.

Valid Configuration file locations:
- `$HOME/.csh-config.yaml`
- `$PWD/.csh-config.yaml`

The syntax highlighting rules are defined in a YAML file. Each rule consists of:
- **`description`**: A human-readable explanation of what the rule does.
- **`regex`**: The pattern to match in SSH output.
- **`color`**: The color to apply, specified as a key from the `palette` section.

### Example Configuration (`csh-config.yaml`)
```yaml
palette:
  green: "#32cd32"
  red: "#ff4500"
  yellow: "#ffd700"
  blue: "#0000ff"

rules:
  - description: "Highlight IP addresses in green"
    regex: |
      (?x)          # Enable free-spacing mode for readability
      \b            # Start of a word boundary
      \d{1,3}       # Match 1-3 digits
      (\.\d{1,3}){3} # Match three ".<1-3 digits>" sequences
      \b            # End of a word boundary
    color: "green"

  - description: "Highlight words related to interfaces"
    regex: |
      (?ix)         # Case-insensitive and free-spacing mode
      \b(
      bgp          # BGP
      )\b
    color: "red"

  - description: "Highlight all URLs in blue"
    regex: https?://[^\s/$.?#].[^\s]*
    color: "blue"
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

