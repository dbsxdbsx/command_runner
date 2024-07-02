# command_runner

`command_runner` is a cross-platform Rust crate designed for executing terminal commands interactively. It wraps various features in a struct to provide a seamless command execution experience.

## TODO
- Test with guessing game(Rust/python script)
- buffer.fill(0); need?
- force temrinate when catch err output?
- change result to anyhow
- get_status ... have to be mut?/ output has to be Vec<>?
- does `get_output` would be consume 1 ele each time after calling?
- let 3 threads be green thread
- distinguish `ExceptionalTerminated` into `forceTerminated` and `inner panic`?

## Key Features

1. **Execute Command Line Instructions**: Run any command line instruction from within your Rust application.
2. **Check Command Execution Status**: Determine if a command executed successfully.
3. **Fetch Command Output**: Retrieve the real-time output of the command, similar to what you would see in a terminal.
4. **Handle User Input**: If a running command requires user input, the crate provides a way to input data easily while still capturing the command's output.
5. **Cross-Platform Compatibility**: Works seamlessly across different platforms, including Linux, macOS, Windows, and mobile platforms like Android.

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
