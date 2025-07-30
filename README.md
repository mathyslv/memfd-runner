# memfd-runner

[![Crates.io](https://img.shields.io/crates/v/memfd-runner.svg)](https://crates.io/crates/memfd-runner)
[![Documentation](https://docs.rs/memfd-runner/badge.svg)](https://docs.rs/memfd-runner)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A minimal Linux library for executing in-memory ELF files using `memfd_create` and `execve`.

## Overview

memfd-runner provides a simple interface to load and execute ELF binaries directly from memory without writing them to disk. It uses Linux's `memfd_create` system call to create an anonymous file in memory, writes the ELF data to it, then executes it via the `/proc/self/fd/` interface.

## Features

- **Minimal** - <400 lines of code, 1 dependency ([syscaller](https://github.com/mathyslv/syscaller))
- **Two execution modes** - fork child process or replace current process
- **`no_std`** - works in embedded and kernel environments  

## Platform Support

- **Linux only** - requires `memfd_create` system call (Linux 3.17+)
- **x86_64** - tested on x86_64 architecture

## Installation

```sh
cargo add mdmfd-runner
```

Or add this to your `Cargo.toml`:

```toml
[dependencies]
memfd-runner = "0.1.0"
```

## Quick Start

### Simple Execution (Fork Mode)

```rust
use memfd_runner::run;

// Read an ELF binary
let elf_bytes = std::fs::read("/usr/bin/echo").unwrap();

// Execute it and get the exit code
let exit_code = run(&elf_bytes).unwrap();
println!("Process exited with code: {}", exit_code);
```

### Replace Current Process

```rust
use memfd_runner::{run_with_options, RunOptions};

let elf_bytes = std::fs::read("/usr/bin/uname").unwrap();
let options = RunOptions::new().with_replace(true);

// This will replace the current process - does not return on success
run_with_options(&elf_bytes, options).unwrap();
```

### Error Handling

```rust
use memfd_runner::{run, RunError};

let invalid_data = b"not an elf file";
match run(invalid_data) {
    Ok(exit_code) => println!("Success: {}", exit_code),
    Err(RunError::InvalidElfFormat) => println!("Invalid ELF format"),
    ...
}
```

## API Reference

### Functions

- **`run<B: AsRef<[u8]>>(bytes: B) -> Result<i32, RunError>`**
  - Execute ELF bytes in fork mode, returns child exit code
  
- **`run_with_options<B: AsRef<[u8]>>(bytes: B, options: RunOptions) -> Result<i32, RunError>`**
  - Execute ELF bytes with custom options

### Types

- **`RunOptions`** - Configuration for execution
  - `new()` - Create default options (fork mode)
  - `with_replace(bool)` - Set replace mode (true = replace process, false = fork child)

- **`RunError`** - Error types with context
  - `FdCreationFailed(i32)` - Failed to create memory file descriptor
  - `BytesNotWritten(usize, usize)` - Write operation failed (written, expected)
  - `ExecError(i32)` - execve system call failed
  - `ForkError(i32)` - fork system call failed  
  - `WaitError(i32)` - wait4 system call failed
  - `InvalidElfFormat` - ELF validation failed

## How It Works

1. **Create Memory FD**: Uses `memfd_create()` to create an anonymous file in memory
3. **Write Data**: Writes the ELF bytes to the memory file descriptor
4. **Execute**: Uses `execve()` with `/proc/self/fd/<fd>` path to execute the in-memory file
5. **Wait for Child**: In fork mode, waits for child process and returns exit code

⚠️ **Limitations**: Very basic ELF validation only - complex validation should be done by caller

## Examples

See the [`examples/`](examples/) directory for complete examples:

- [`example.rs`](examples/example.rs) - Basic usage demonstration

## Development

### Building

```bash
cargo build
```

### Testing

```bash
cargo test
```

### Linting

```bash
cargo check
cargo clippy
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Changelog

### 0.1.0
- Initial release
- Basic memfd_create + execve functionality
- Fork and replace execution modes
- ELF validation
- Comprehensive error handling
