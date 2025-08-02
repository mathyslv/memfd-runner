# memfd-runner

[![Crates.io](https://img.shields.io/crates/v/memfd-runner.svg)](https://crates.io/crates/memfd-runner)
[![Documentation](https://docs.rs/memfd-runner/badge.svg)](https://docs.rs/memfd-runner)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/mathyslv/memfd-runner)

A minimal Linux library for executing in-memory ELF files using `memfd_create` and `execve`.

## Overview

memfd-runner provides a simple interface to load and execute ELF binaries directly from memory without writing them to disk. It uses Linux's `memfd_create` system call to create an anonymous file in memory, writes the ELF data to it, then executes it via the `/proc/self/fd/` interface.

## Features

- **Minimal** - <400 lines of code, 1 dependency ([syscaller](https://github.com/mathyslv/syscaller))
- **Two execution modes** - fork child process or replace current process
- **Command line arguments** - pass custom arguments to executed programs
- **Environment variables** - set custom environment for executed programs
- **Custom argv[0]** - control how the program sees its own name
- **`no_std`** - works in embedded and kernel environments
- **Basic ELF validation** - validates magic bytes and minimum size

## Platform Support

- **Linux only** - requires `memfd_create` system call (Linux 3.17+)
- **x86_64** - tested on x86_64 architecture

## Installation

```sh
cargo add memfd-runner
```

Or add this to your `Cargo.toml`:

```toml
[dependencies]
memfd-runner = "0.1.1"
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

### Passing Arguments

```rust
use memfd_runner::{run_with_options, RunOptions};

let elf_bytes = std::fs::read("/usr/bin/echo").unwrap();
let options = RunOptions::new()
    .with_args(&["Hello", "World!"]);  // Just the arguments, not the program name

let exit_code = run_with_options(&elf_bytes, options).unwrap();
// Executes: /proc/self/fd/X "Hello" "World!"
```

### Custom Program Name (argv[0])

```rust
use memfd_runner::{run_with_options, RunOptions};

let elf_bytes = std::fs::read("/usr/bin/echo").unwrap();
let options = RunOptions::new()
    .with_argv0("my-echo")  // Custom program name
    .with_args(&["Hello", "World!"]);

let exit_code = run_with_options(&elf_bytes, options).unwrap();
// The program sees argv[0] as "my-echo" instead of "/proc/self/fd/X"
```

### Environment Variables

```rust
use memfd_runner::{run_with_options, RunOptions};

let elf_bytes = std::fs::read("/usr/bin/env").unwrap();
let options = RunOptions::new()
    .with_env(&["PATH=/usr/bin", "HOME=/tmp"]);

let exit_code = run_with_options(&elf_bytes, options).unwrap();
```

### Error Handling

```rust
use memfd_runner::{run, RunError};

let invalid_data = b"not an elf file";
match run(invalid_data) {
    Ok(exit_code) => println!("Success: {}", exit_code),
    Err(RunError::InvalidElfFormat) => println!("Invalid ELF format"),
    Err(RunError::FdCreationFailed(errno)) => println!("Failed to create memfd: {}", errno),
    Err(RunError::TooManyArgs) => println!("Too many arguments provided"),
    Err(e) => println!("Other error: {:?}", e),
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
  - `with_args(&[&str])` - Set command line arguments (max 32 args, 256 chars each)
  - `with_env(&[&str])` - Set environment variables (max 64 vars, 256 chars each)
  - `with_argv0(&str)` - Set custom program name (argv[0])

- **`RunError`** - Error types with context
  - `FdCreationFailed(i32)` - Failed to create memory file descriptor
  - `BytesNotWritten(usize, usize)` - Write operation failed (written, expected)
  - `ExecError(i32)` - execve system call failed
  - `ForkError(i32)` - fork system call failed  
  - `WaitError(i32)` - wait4 system call failed
  - `InvalidElfFormat` - ELF validation failed
  - `TooManyArgs` - Too many command line arguments (limit: 32)
  - `TooManyEnvVars` - Too many environment variables (limit: 64)
  - `ArgTooLong` - Command line argument too long (limit: 256 chars)
  - `EnvVarTooLong` - Environment variable too long (limit: 256 chars)

## How It Works

1. **Validate ELF**: Checks magic bytes (0x7f, 'E', 'L', 'F') and minimum size
2. **Create Memory FD**: Uses `memfd_create()` to create an anonymous file in memory
3. **Write Data**: Writes the ELF bytes to the memory file descriptor
4. **Prepare Arguments**: Builds argv and envp arrays with provided options
5. **Execute**: Uses `execve()` with `/proc/self/fd/<fd>` path to execute the in-memory file
6. **Wait for Child**: In fork mode, waits for child process and returns exit code

## Limitations

- **Linux-specific** - requires `memfd_create` system call (Linux 3.17+)
- **Maximum 32 command line arguments** (256 characters each)
- **Maximum 64 environment variables** (256 characters each)  
- **Basic ELF validation only** - validates magic bytes and minimum size
- **No complex ELF features** - no support for dynamic linking validation

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
5. Run `cargo test` and `cargo clippy`
6. Submit a pull request

## License

This project is dual-licensed under the MIT OR Apache-2.0 license. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE) files for details.