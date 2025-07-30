//! # memfd-runner
//!
//! A minimal Linux library for executing in-memory ELF files using `memfd_create` and `execve`.
//!
//! This library provides a simple interface to load and execute ELF binaries directly from memory
//! without writing them to disk. It uses Linux's `memfd_create` system call to create an anonymous
//! file in memory, writes the ELF data to it, then executes it via the `/proc/self/fd/` interface.
//!
//! ## Features
//!
//! - **Minimal** - <400 lines of code, 1 dependency ([syscaller](https://github.com/mathyslv/syscaller))
//! - **Two execution modes** - fork child process or replace current process
//! - **`no_std`** - works in embedded and kernel environments  
//!
//! ## Platform Support
//!
//! - **Linux only** - requires `memfd_create` system call (Linux 3.17+)
//! - **x86_64** - tested on x86_64 architecture
//!
//! ## Usage
//!
//! ### Simple execution (fork mode)
//!
//! ```rust,no_run
//! use memfd_runner::run;
//!
//! let elf_bytes = std::fs::read("/usr/bin/echo").unwrap();
//! let exit_code = run(&elf_bytes).unwrap();
//! println!("Process exited with code: {}", exit_code);
//! ```
//!
//! ### Replace current process
//!
//! ```rust,no_run
//! use memfd_runner::{run_with_options, RunOptions};
//!
//! let elf_bytes = std::fs::read("/usr/bin/uname").unwrap();
//! let options = RunOptions::new().with_replace(true);
//! run_with_options(&elf_bytes, options).unwrap(); // Does not return
//! ```
//!
//! ### Passing arguments and environment variables
//!
//! ```rust,no_run
//! use memfd_runner::{run_with_options, RunOptions};
//!
//! let elf_bytes = std::fs::read("/usr/bin/echo").unwrap();
//! let options = RunOptions::new()
//!     .with_args(&["Hello", "World!"])  // Just the arguments, not the program name
//!     .with_env(&["PATH=/usr/bin", "HOME=/tmp"]);
//! let exit_code = run_with_options(&elf_bytes, options).unwrap();
//! // Executes: /proc/self/fd/X "Hello" "World!"
//! ```
//!
//! ### Custom argv[0] (program name)
//!
//! ```rust,no_run
//! use memfd_runner::{run_with_options, RunOptions};
//!
//! let elf_bytes = std::fs::read("/usr/bin/echo").unwrap();
//! let options = RunOptions::new()
//!     .with_argv0("my-echo")  // Custom program name
//!     .with_args(&["Hello", "World!"]);
//! let exit_code = run_with_options(&elf_bytes, options).unwrap();
//! // The program sees argv[0] as "my-echo" instead of "/proc/self/fd/X"
//! ```
//!
//! ### Error handling
//!
//! ```rust,no_run
//! use memfd_runner::{run, RunError};
//!
//! let invalid_elf = b"not an elf file";
//! match run(invalid_elf) {
//!     Ok(exit_code) => println!("Success: {}", exit_code),
//!     Err(RunError::InvalidElfFormat) => println!("Invalid ELF format"),
//!     Err(RunError::FdCreationFailed(errno)) => println!("Failed to create memfd: {}", errno),
//!     Err(e) => println!("Other error: {:?}", e),
//! }
//! ```
//!
//! ## Limitations
//!
//! - Linux-specific
//! - Maximum 32 command line arguments (256 chars each)
//! - Maximum 64 environment variables (256 chars each)
//! - Very basic ELF validation only (magic bytes, minimum size)
//! - No support for complex ELF features or dynamic linking validation

#![no_std]

mod syscalls;

const MFD_CLOEXEC: u8 = 0x1;

#[used]
pub static EMPTY_STRING: [u8; 8] = [0; 8];

/// Error types returned by memfd-runner operations.
#[derive(Debug)]
pub enum RunError {
    /// Failed to create memory file descriptor via memfd_create()
    FdCreationFailed(i32),
    /// Failed to write all ELF bytes to memory file
    BytesNotWritten(usize, usize),
    /// execve() system call failed
    ExecError(i32),
    /// fork() system call failed
    ForkError(i32),
    /// wait4() system call failed while waiting for child process
    WaitError(i32),
    /// ELF validation failed - invalid magic bytes or insufficient size
    InvalidElfFormat,
    /// Too many command line arguments provided (limit: 32)
    TooManyArgs,
    /// Too many environment variables provided (limit: 64)
    TooManyEnvVars,
    /// Command line argument too long (limit: 256 characters)
    ArgTooLong,
    /// Environment variable too long (limit: 256 characters)
    EnvVarTooLong,
}

const MAX_ARGS: usize = 32;
const MAX_ENV: usize = 64;
const MAX_STRING_LEN: usize = 256;

/// Options which can be used to customize arguments, environment and how the ELF is executed.
#[derive(Clone, Default)]
pub struct RunOptions<'a> {
    replace: bool,
    args: Option<&'a [&'a str]>,
    env: Option<&'a [&'a str]>,
    argv0: Option<&'a str>,
}

impl<'a> RunOptions<'a> {
    /// Creates a blank new set of options ready for configuration.
    ///
    /// All options are initially empty / set to false.
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggles the replace mode. If set to `true`, the current process will be replaced by the executed binary.
    /// Otherwise, `fork()` will be called and the current process will be able to wait for the child.
    pub fn with_replace(mut self, replace: bool) -> Self {
        self.replace = replace;
        self
    }

    /// Set command line arguments for the executed binary.
    ///
    /// These are the actual arguments passed to the program (argv[1], argv[2], etc.).
    /// The program name (argv[0]) is automatically set to the memfd path.
    ///
    /// # Example
    /// ```rust,no_run
    /// use memfd_runner::{run_with_options, RunOptions};
    ///
    /// let elf_bytes = std::fs::read("/usr/bin/echo").unwrap();
    /// let options = RunOptions::new().with_args(&["Hello", "World!"]);
    /// let exit_code = run_with_options(&elf_bytes, options).unwrap();
    /// // This executes: /proc/self/fd/X "Hello" "World!"
    /// ```
    pub fn with_args(mut self, args: &'a [&'a str]) -> Self {
        self.args = Some(args);
        self
    }

    /// Set environment variables for the executed binary.
    /// Environment variables should be in "KEY=value" format.
    ///
    /// # Example
    /// ```rust,no_run
    /// use memfd_runner::{run_with_options, RunOptions};
    ///
    /// let elf_bytes = std::fs::read("/usr/bin/env").unwrap();
    /// let options = RunOptions::new().with_env(&["PATH=/usr/bin", "HOME=/tmp"]);
    /// let exit_code = run_with_options(&elf_bytes, options).unwrap();
    /// ```
    pub fn with_env(mut self, env: &'a [&'a str]) -> Self {
        self.env = Some(env);
        self
    }

    /// Set a custom argv[0] for the executed binary.
    ///
    /// By default, argv[0] is set to the memfd path (`/proc/self/fd/N`). This method
    /// allows you to customize what the executed program sees as its program name.
    ///
    /// # Example
    /// ```rust,no_run
    /// use memfd_runner::{run_with_options, RunOptions};
    ///
    /// let elf_bytes = std::fs::read("/usr/bin/echo").unwrap();
    /// let options = RunOptions::new()
    ///     .with_argv0("my-custom-program")
    ///     .with_args(&["Hello", "World!"]);
    /// let exit_code = run_with_options(&elf_bytes, options).unwrap();
    /// // The program sees argv[0] as "my-custom-program"
    /// ```
    pub fn with_argv0(mut self, argv0: &'a str) -> Self {
        self.argv0 = Some(argv0);
        self
    }
}

/// Executes an in-memory ELF binary by creating a child process.
///
/// This is the simplest way to execute an ELF binary from memory. It does a very basic ELF header verification,
/// creates a memory file descriptor, writes the ELF data to it, then forks a child process
/// and executes the binary via `/proc/self/fd/{fd}`.
///
/// # Arguments
///
/// * `bytes` - The ELF binary data to execute (must have valid ELF magic bytes)
///
/// # Returns
///
/// * `Ok(exit_code)` - The exit code of the executed process (0-255)
/// * `Err(RunError)` - Various error conditions during execution
///
/// # Examples
///
/// ```rust,no_run
/// // Execute /usr/bin/ls from memory
/// let elf_bytes = std::fs::read("/usr/bin/ls").unwrap();
/// let exit_code = memfd_runner::run(&elf_bytes).unwrap();
/// println!("Process exited with code: {}", exit_code);
/// ```
pub fn run<B: AsRef<[u8]>>(bytes: B) -> Result<i32, RunError> {
    run_with_options(bytes, RunOptions::default())
}

/// Executes an in-memory ELF binary with configurable options.
///
/// This function provides more control over the execution process compared to [`run`].
/// It supports both fork mode (default) and replace mode where the current process
/// is replaced by the executed binary.
///
/// # Arguments
///
/// * `bytes` - The ELF binary data to execute (must have valid ELF magic bytes)
/// * `options` - Configuration options for execution behavior
///
/// # Returns
///
/// * `Ok(exit_code)` - The exit code of the executed process (fork mode only)
/// * `Err(RunError)` - Various error conditions during execution
/// * **Never returns** in `replace` mode on successful execution
///
/// # Examples
///
/// ```rust,no_run
/// use memfd_runner::{run_with_options, RunOptions};
///
/// let elf_bytes = std::fs::read("/usr/bin/uname").unwrap();
/// // replace mode
/// let options = RunOptions::new().with_replace(true);
/// run_with_options(&elf_bytes, options).unwrap(); // never returns
/// unreachable!("this line will never execute");
/// ```
pub fn run_with_options<B: AsRef<[u8]>>(
    bytes: B,
    options: RunOptions<'_>,
) -> Result<i32, RunError> {
    let fd = create_fd()?;
    let bytes = bytes.as_ref();
    write_bytes(fd, bytes)?;
    execute(fd, options)
}

fn create_fd() -> Result<u16, RunError> {
    // Safety: EMPTY_STRING is a valid null-terminated string
    let fd = unsafe { syscalls::memfd_create(EMPTY_STRING, MFD_CLOEXEC as u32) };
    if fd == -1 {
        return Err(RunError::FdCreationFailed(-1)); // TODO: get actual errno
    }
    Ok(fd as _)
}

fn validate_elf_header(bytes: &[u8]) -> bool {
    // Check minimum header size
    if bytes.len() < 16 {
        return false;
    }
    // Check ELF magic: 0x7f, 'E', 'L', 'F'
    bytes[0] == 0x7f && bytes[1] == b'E' && bytes[2] == b'L' && bytes[3] == b'F'
}

fn write_bytes(fd: u16, bytes: &[u8]) -> Result<(), RunError> {
    if !validate_elf_header(bytes) {
        unsafe { syscalls::close(fd as i32) };
        return Err(RunError::InvalidElfFormat);
    }
    let written = unsafe { syscalls::write(fd as _, bytes.as_ptr().cast_mut(), bytes.len()) };
    if written != bytes.len() as _ {
        unsafe { syscalls::close(fd as i32) };
        return Err(RunError::BytesNotWritten(written as usize, bytes.len()));
    }
    Ok(())
}

pub struct PreparedArgs {
    ptrs: [*const u8; MAX_ARGS + 1],
    storage: [[u8; MAX_STRING_LEN]; MAX_ARGS],
}

impl PreparedArgs {
    /// Get the pointer to the argv array for execve
    pub fn as_ptr(&self) -> *const *const u8 {
        self.ptrs.as_ptr()
    }
}

pub struct PreparedEnv {
    ptrs: [*const u8; MAX_ENV + 1],
    storage: [[u8; MAX_STRING_LEN]; MAX_ENV],
}

impl PreparedEnv {
    pub fn as_ptr(&self) -> *const *const u8 {
        self.ptrs.as_ptr()
    }
}

fn prepare_argv(fd: u16, options: &RunOptions<'_>) -> Result<PreparedArgs, RunError> {
    let mut prepared = PreparedArgs {
        ptrs: [core::ptr::null(); MAX_ARGS + 1],
        storage: [[0u8; MAX_STRING_LEN]; MAX_ARGS],
    };

    // Set argv[0] - either custom or default memfd path
    if let Some(custom_argv0) = options.argv0 {
        let argv0_bytes = custom_argv0.as_bytes();
        if argv0_bytes.len() >= MAX_STRING_LEN {
            return Err(RunError::ArgTooLong);
        }
        prepared.storage[0][..argv0_bytes.len()].copy_from_slice(argv0_bytes);
        prepared.storage[0][argv0_bytes.len()] = 0; // null terminate
    } else {
        // Use default memfd path
        let path = build_path(fd);
        let null_pos = path.iter().position(|&b| b == 0).unwrap();
        prepared.storage[0][..null_pos].copy_from_slice(&path[..null_pos]);
        prepared.storage[0][null_pos] = 0; // ensure null termination
    }
    prepared.ptrs[0] = prepared.storage[0].as_ptr();

    // Add user-provided arguments
    if let Some(user_args) = options.args {
        if user_args.len() > MAX_ARGS - 1 {
            return Err(RunError::TooManyArgs);
        }

        for &arg in user_args.iter() {
            let arg_bytes = arg.as_bytes();
            if arg_bytes.len() >= MAX_STRING_LEN {
                return Err(RunError::ArgTooLong);
            }

            prepared.storage[arg_count][..arg_bytes.len()].copy_from_slice(arg_bytes);
            prepared.storage[arg_count][arg_bytes.len()] = 0; // null terminate
            prepared.ptrs[arg_count] = prepared.storage[arg_count].as_ptr();
            arg_count += 1;
        }
    }

    Ok(prepared)
}

fn prepare_envp(env: Option<&[&str]>) -> Result<PreparedEnv, RunError> {
    let mut prepared = PreparedEnv {
        ptrs: [core::ptr::null(); MAX_ENV + 1],
        storage: [[0u8; MAX_STRING_LEN]; MAX_ENV],
    };

    if let Some(user_env) = env {
        if user_env.len() > MAX_ENV {
            return Err(RunError::TooManyEnvVars);
        }

        for (i, &env_var) in user_env.iter().enumerate() {
            let env_bytes = env_var.as_bytes();
            if env_bytes.len() >= MAX_STRING_LEN {
                return Err(RunError::EnvVarTooLong);
            }

            prepared.storage[i][..env_bytes.len()].copy_from_slice(env_bytes);
            prepared.storage[i][env_bytes.len()] = 0; // null terminate
            prepared.ptrs[i] = prepared.storage[i].as_ptr();
        }
    }

    Ok(prepared)
}

fn execute(fd: u16, options: RunOptions<'_>) -> Result<i32, RunError> {
    let pid = match options.replace {
        true => 0, // simulate we are the child
        false => unsafe { syscalls::fork() },
    };

    // if child, call execve
    match pid {
        0 => {
            let path = build_path(fd);
            let argv = prepare_argv(fd, &options)?;
            let envp = prepare_envp(options.env)?;

            let ret = unsafe {
                syscalls::execve(path, argv.as_ptr() as *mut u8, envp.as_ptr() as *mut u8)
            };
            if ret == -1 {
                return Err(RunError::ExecError(-1)); // TODO: get actual errno
            }
            unreachable!("execve should not return on success");
        }
        -1 => Err(RunError::ForkError(-1)), // TODO: get actual errno
        _ => {
            let mut status: i32 = 0;
            let waited_pid = unsafe {
                syscalls::wait4(
                    pid,
                    &mut status as *mut i32 as *mut u8,
                    0,
                    core::ptr::null_mut(),
                )
            };
            if waited_pid == -1 {
                return Err(RunError::WaitError(-1)); // TODO: get actual errno
            }
            // Extract exit code using WEXITSTATUS equivalent: (status >> 8) & 0xff
            Ok((status >> 8) & 0xff)
        }
    }
}

const EXEC_PATH: [u8; 20] = *b"/proc/self/fd/\0\0\0\0\0\0";
const EXEC_PATH_LEN: usize = EXEC_PATH.len();

fn build_path(fd: u16) -> [u8; EXEC_PATH_LEN] {
    let mut path = [0u8; EXEC_PATH_LEN];

    unsafe { core::ptr::copy_nonoverlapping(EXEC_PATH.as_ptr(), path.as_mut_ptr(), EXEC_PATH_LEN) };
    let mut idx = 14;
    if fd >= 10000 {
        path[idx] = b'0' + (fd / 10000) as u8;
        idx += 1;
    }
    if fd >= 1000 {
        path[idx] = b'0' + ((fd / 1000) % 10) as u8;
        idx += 1;
    }
    if fd >= 100 {
        path[idx] = b'0' + ((fd / 100) % 10) as u8;
        idx += 1;
    }
    if fd >= 10 {
        path[idx] = b'0' + ((fd / 10) % 10) as u8;
        idx += 1;
    }
    path[idx] = b'0' + (fd % 10) as u8;
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    use std::format;

    // RunOptions Tests
    #[test]
    fn test_run_options_default() {
        let options = RunOptions::new();
        assert!(!options.replace);
        assert!(options.args.is_none());
        assert!(options.env.is_none());
    }

    #[test]
    fn test_run_options_with_replace() {
        let options = RunOptions::new().with_replace(true);
        assert!(options.replace);
    }

    #[test]
    fn test_run_options_with_args() {
        let args = ["test", "arg1", "arg2"];
        let options = RunOptions::new().with_args(&args);
        assert!(options.args.is_some());
        assert_eq!(options.args.unwrap().len(), 3);
        assert_eq!(options.args.unwrap()[0], "test");
        assert_eq!(options.args.unwrap()[1], "arg1");
        assert_eq!(options.args.unwrap()[2], "arg2");
    }

    #[test]
    fn test_run_options_with_env() {
        let env = ["PATH=/usr/bin", "HOME=/tmp"];
        let options = RunOptions::new().with_env(&env);
        assert!(options.env.is_some());
        assert_eq!(options.env.unwrap().len(), 2);
        assert_eq!(options.env.unwrap()[0], "PATH=/usr/bin");
        assert_eq!(options.env.unwrap()[1], "HOME=/tmp");
    }

    #[test]
    fn test_run_options_chaining() {
        let args = ["test", "arg1"];
        let env = ["VAR=value"];
        let options = RunOptions::new()
            .with_replace(true)
            .with_args(&args)
            .with_env(&env);

        assert!(options.replace);
        assert!(options.args.is_some());
        assert!(options.env.is_some());
        assert_eq!(options.args.unwrap().len(), 2);
        assert_eq!(options.env.unwrap().len(), 1);
    }

    #[test]
    fn test_run_options_with_argv0() {
        let options = RunOptions::new().with_argv0("custom-program");
        assert!(options.argv0.is_some());
        assert_eq!(options.argv0.unwrap(), "custom-program");
    }

    #[test]
    fn test_run_options_argv0_chaining() {
        let args = ["test", "arg1"];
        let env = ["VAR=value"];
        let options = RunOptions::new()
            .with_argv0("my-program")
            .with_args(&args)
            .with_env(&env);

        assert!(options.argv0.is_some());
        assert_eq!(options.argv0.unwrap(), "my-program");
        assert!(options.args.is_some());
        assert!(options.env.is_some());
    }

    // prepare_argv Tests
    #[test]
    fn test_prepare_argv_path_only() {
        let options = RunOptions::new();
        let result = prepare_argv(123, &options);
        assert!(result.is_ok());

        let argv = result.unwrap();
        assert!(!argv.ptrs[0].is_null());
        assert!(argv.ptrs[1].is_null()); // null terminated
    }

    #[test]
    fn test_prepare_argv_with_args() {
        let args = ["arg1", "arg2"];
        let options = RunOptions::new().with_args(&args);
        let result = prepare_argv(123, &options);
        assert!(result.is_ok());

        let argv = result.unwrap();
        assert!(!argv.ptrs[0].is_null()); // path
        assert!(!argv.ptrs[1].is_null()); // arg1
        assert!(!argv.ptrs[2].is_null()); // arg2
        assert!(argv.ptrs[3].is_null()); // null terminated
    }

    #[test]
    fn test_prepare_argv_too_many_args() {
        let mut args = std::vec::Vec::new();
        for _i in 0..MAX_ARGS {
            args.push("arg");
        }
        let options = RunOptions::new().with_args(&args);
        let result = prepare_argv(123, &options);
        assert!(matches!(result, Err(RunError::TooManyArgs)));
    }

    #[test]
    fn test_prepare_argv_arg_too_long() {
        let long_arg = "a".repeat(MAX_STRING_LEN);
        let args = [long_arg.as_str()];
        let options = RunOptions::new().with_args(&args);
        let result = prepare_argv(123, &options);
        assert!(matches!(result, Err(RunError::ArgTooLong)));
    }

    #[test]
    fn test_prepare_argv_custom_argv0() {
        let options = RunOptions::new().with_argv0("my-custom-program");
        let result = prepare_argv(123, &options);
        assert!(result.is_ok());

        let argv = result.unwrap();
        assert!(!argv.ptrs[0].is_null());
        assert!(argv.ptrs[1].is_null()); // null terminated

        // Check that argv[0] contains our custom string
        let argv0_str = unsafe {
            let ptr = argv.ptrs[0] as *const u8;
            let mut len = 0;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            std::str::from_utf8(std::slice::from_raw_parts(ptr, len)).unwrap()
        };
        assert_eq!(argv0_str, "my-custom-program");
    }

    #[test]
    fn test_prepare_argv_custom_argv0_with_args() {
        let args = ["arg1", "arg2"];
        let options = RunOptions::new().with_argv0("custom-name").with_args(&args);
        let result = prepare_argv(123, &options);
        assert!(result.is_ok());

        let argv = result.unwrap();
        assert!(!argv.ptrs[0].is_null()); // custom argv0
        assert!(!argv.ptrs[1].is_null()); // arg1
        assert!(!argv.ptrs[2].is_null()); // arg2
        assert!(argv.ptrs[3].is_null()); // null terminated
    }

    #[test]
    fn test_prepare_argv_custom_argv0_too_long() {
        let long_argv0 = "a".repeat(MAX_STRING_LEN);
        let options = RunOptions::new().with_argv0(&long_argv0);
        let result = prepare_argv(123, &options);
        assert!(matches!(result, Err(RunError::ArgTooLong)));
    }

    // prepare_envp Tests
    #[test]
    fn test_prepare_envp_none() {
        let result = prepare_envp(None);
        assert!(result.is_ok());

        let envp = result.unwrap();
        assert!(envp.ptrs[0].is_null()); // immediately null terminated
    }

    #[test]
    fn test_prepare_envp_with_env() {
        let env = ["PATH=/usr/bin", "HOME=/tmp"];
        let result = prepare_envp(Some(&env));
        assert!(result.is_ok());

        let envp = result.unwrap();
        assert!(!envp.ptrs[0].is_null()); // PATH
        assert!(!envp.ptrs[1].is_null()); // HOME
        assert!(envp.ptrs[2].is_null()); // null terminated
    }

    #[test]
    fn test_prepare_envp_too_many_vars() {
        let mut env = std::vec::Vec::new();
        for _i in 0..MAX_ENV + 1 {
            env.push("VAR=value");
        }
        let result = prepare_envp(Some(&env));
        assert!(matches!(result, Err(RunError::TooManyEnvVars)));
    }

    #[test]
    fn test_prepare_envp_var_too_long() {
        let long_var = format!("VAR={}", "a".repeat(MAX_STRING_LEN));
        let env = [long_var.as_str()];
        let result = prepare_envp(Some(&env));
        assert!(matches!(result, Err(RunError::EnvVarTooLong)));
    }

    // New error types tests
    #[test]
    fn test_new_error_types() {
        let errors = [
            RunError::TooManyArgs,
            RunError::TooManyEnvVars,
            RunError::ArgTooLong,
            RunError::EnvVarTooLong,
        ];

        for error in &errors {
            let debug_str = format!("{error:?}");
            assert!(!debug_str.is_empty());
        }
    }
}
