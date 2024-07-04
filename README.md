# command_runner
 **NOT STABLE YET! DON'T USE IT!**
`command_runner` is a cross-platform Rust crate designed for executing terminal commands interactively. It wraps various features in a struct to provide a seamless command execution experience.

## TODO
- no init stdin, but do so when it is for ok for input?
- distinguish `ExceptionalTerminated` into `forceTerminated` and `inner panic`?
- let 3 threads be green thread
- Test with guessing game(Rust/python script)

## Key Features

1. **Command Execution**: Run any command line instruction directly from your Rust application.
2. **Execution Status Checking**: Easily determine the success or failure of executed commands.
3. **Real-time Output Capture**: Retrieve and monitor command output in real-time, mirroring terminal behavior.
4. **Interactive Input Handling**: Seamlessly provide user input for commands that require it, while still capturing output.
5. **Cross-Platform Support**: Function consistently across Linux, macOS, Windows, and mobile platforms like Android.
6. **Efficient Concurrency with Green Threads**: Utilize lightweight green threads for efficient concurrent execution without the overhead of OS thread creation. This feature integrates smoothly without requiring explicit runtime usage, offering flexibility and runtime-agnostic concurrency.
7. **no_std Compatibility**: Operate in environments without the standard library, enhancing versatility across various contexts.
8. **Simplified Command Line Interface**: Streamline CLI operations in Rust applications with an easy-to-use, feature-rich crate.

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
