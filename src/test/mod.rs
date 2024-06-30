#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn test_os_built_in_command() {
        let ping_count_option = if cfg!(target_os = "windows") {
            "-n"
        } else {
            "-c"
        };

        let check_num = 2;
        let mut executor = CommandRunner::run(
            "ping",
            &[ping_count_option, &check_num.to_string(), "google.com"],
        )
        .unwrap();

        let mut output_count = 0;
        loop {
            match executor.get_status() {
                CommandStatus::Running => {
                    let output = executor.get_output();
                    if !output.is_empty() {
                        output_count += output.len();
                        println!("Current Output:");
                        for line in output {
                            println!("{}", line);
                        }
                    }

                    let error = executor.get_error();
                    if !error.is_empty() {
                        println!("Current Error:");
                        for line in error {
                            println!("{}", line);
                        }
                        panic!("There should not be error in this test case!")
                    }
                }
                CommandStatus::Finished => {
                    println!("Built-in Command completed successfully");
                    break;
                }
                CommandStatus::ExceptionTerminated => {
                    panic!("Built-in Command terminated with error");
                }
            }
        }

        assert!(
            output_count > check_num,
            "Expected output count to be greater than {}, but got {}",
            check_num,
            output_count
        );
        println!("Total output lines: {}", output_count);
    }

    #[test]
    fn test_customized_app_command() {
        let mut executor = CommandRunner::run("./customized_app", &[]).unwrap();

        let mut all_output = Vec::new();
        loop {
            match executor.get_status() {
                CommandStatus::Running => {
                    // collect output
                    let output = executor.get_output();
                    all_output.extend(output);
                    // check output error
                    let error = executor.get_error();
                    assert!(error.is_empty(), "Unexpected error output: {:?}", error);
                }
                CommandStatus::Finished => {
                    println!("Custom application command execution completed");
                    break;
                }
                CommandStatus::ExceptionTerminated => {
                    panic!("Custom application command execution error");
                }
            }
        }

        assert_eq!(
            all_output.len(),
            3,
            "Expected output should have 3 lines, but got {} lines",
            all_output.len()
        );

        // Print the collected output for debugging
        println!("collected output:");
        for (i, line) in all_output.iter().enumerate() {
            println!("line-{}: {}", i + 1, line);
        }
    }

    #[test]
    fn test_invalid_command() {
        let result = CommandRunner::run("non_existent_command", &[]);
        assert!(result.is_err(), "Expected an error for invalid command");
    }

    use std::time::Duration;

    #[test]
    fn test_terminate() {
        // Create a command that outputs continuously
        #[cfg(unix)]
        let (command, args) = ("yes", vec["test"]);
        #[cfg(windows)]
        let (command, args) = ("ping", vec!["-t", "127.0.0.1"]);

        // Create a CommandExecutor instance
        let mut executor =
            CommandRunner::run(command, &args).expect("Failed to create CommandExecutor");

        // Wait for a short time to ensure the command starts executing
        std::thread::sleep(Duration::from_millis(100));

        // Get some initial output
        let initial_output = executor.get_output();
        assert!(
            !initial_output.is_empty(),
            "There should be some initial output"
        );

        // Call the terminate method
        executor.terminate();

        // Get the output again
        std::thread::sleep(Duration::from_millis(100));
        let final_output = executor.get_output();

        // Assertions:
        // 1. The final output should not be much longer than the initial output (allowing for some buffer output)
        assert!(
            final_output.len() <= initial_output.len() + 10,
            "There should not be too much new output after termination"
        );

        // 2. The process status should be ExceptionTerminated
        let status = executor.get_status();
        assert!(
            matches!(status, CommandStatus::ExceptionTerminated),
            "The process should have terminated"
        );

        // 3. The thread handles should be empty because they should have been joined
        assert!(
            executor.thread_handles.is_empty(),
            "All threads should have been joined"
        );
    }
}
