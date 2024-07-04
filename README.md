# command_runner
 **NOT STABLE YET! DON'T USE IT!**
`command_runner` is a cross-platform Rust crate designed for executing terminal commands interactively. It wraps various features in a struct to provide a seamless command execution experience.

## TODO
- no init stdin, but do so when it is for ok for input?
- distinguish `ExceptionalTerminated` into `forceTerminated` and `inner panic`?
- let 3 threads be green thread
- Test with guessing game(Rust/python script)

## Key Features

1. **Command Execution**: Execute any command line instruction directly within your Rust application.
2. **Execution Status Checking**: Easily check if executed commands succeeded or failed.
3. **Real-time Output Capture**: Capture and monitor command output in real-time, mimicking terminal behavior.
4. **Interactive Input Handling**: Provide user input for commands that require it while capturing output.
5. **Cross-Platform Support**: Consistent functionality across Linux, macOS, Windows, and mobile platforms like Android.
6. **Efficient Concurrency with Green Threads**: Use lightweight green threads for efficient concurrent execution without OS thread overhead, integrating smoothly without explicit runtime usage.
7. **no_std Compatibility**: Operate in environments without the standard library, enhancing versatility.
8. **Unified Struct and Easy-to-Use Interface**: Simplify CLI operations with a single, easy-to-use struct and interfaces like `get_status()`, `get_output()`, `get_error()`, and `input("send your input when the command asks for")`.
9. **Non-Blocking I/O**: Perform all I/O operations asynchronously, preventing blocking and enhancing responsiveness.

## Exported Interfaces

`fn run(command: &str) -> Result<Self>`
`fn terminate(&mut self)`
`fn get_status(&mut self) -> CommandStatus`
`fn input(&self, input: &str) -> Result<()>`
`fn get_output(&self) -> Option<String>`
`fn get_error(&self) -> Option<String>`

## Command Status
```rust
pub enum CommandStatus {
    Running,               // the command is valid initialized and is running
    ExitedWithOkStatus,    // exit with success
    ExceptionalTerminated, // exit with failure  TODO: furthur split into `ForceTerminated` and `ExitedPanic`?
    WaitingInput,          // the command reqeust input when it is running
}
```

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
command_runner = "*"



## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
command_runner = "*"
```

## Usage Example

```rust
use anyhow::Result;
use command_runner::CommandRunner;

fn main() -> Result<()> {
    let mut runner = CommandRunner::new("echo Hello, World!", 1024)?;
    // Check if the command was executed successfully
    if runner.get_status() == CommandStatus::Terminated {
        println!("Command executed successfully!");
    }
    // Get the command output
    while let Some(output) = runner.get_output() {
        println!("Command output: {}", output);
    }
    // Handle user input
    runner.execute("read -p 'Enter your name: ' name && echo \"Hello, $name\"")?;
    runner.input_when_running("John Doe\n")?;
    // Get the final output
    while let Some(final_output) = runner.get_output() {
        println!("Final output: {}", final_output);
    }

    Ok(())
}
```

## Documentation

For more detailed usage instructions and API documentation, please visit [docs.rs](https://docs.rs/command_runner).

## Contribution

We welcome issues and pull requests. For major changes, please open an issue first to discuss what you would like to change.

## License

This crate is dual-licensed under either:

- Apache License, Version 2.0 (http://www.apache.org/licenses/LICENSE-2.0)
- MIT license (http://opensource.org/licenses/MIT)

Choose the license that best fits your needs.
