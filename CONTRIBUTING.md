## Contributing

Contributions are welcomed just please make sure to do the following before opening a pull request.

```bash
# Build and test locally
cargo build --release
./target/release/colorsh user@testhost

# Run linter
cargo clippy

# Format code
cargo fmt
```

### Open a Pull Request

- Provide a clear description of your changes
- Reference any related issues
- Ensure CI/CD checks pass