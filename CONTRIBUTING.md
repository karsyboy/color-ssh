## Contributing

Before opening a pull request, run the full local verification suite.

```bash
# Format check
cargo fmt --all --check

# Lint
cargo clippy --all-targets -- -D warnings

# Tests
cargo test --all-targets
```

### Open a Pull Request

- Provide a clear description of your changes
- Reference any related issues
- Ensure CI checks pass
