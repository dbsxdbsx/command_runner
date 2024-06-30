#[cfg(test)]
mod tests {
    use crate::*;

    #[tokio::test]
    async fn test_os_built_in_command() {
        let ping_count_option = if cfg!(target_os = "windows") {
            "-n"
        } else {
            "-c"
        };

        let mut executor = CommandExecutor::new("ping", &[ping_count_option, "1", "google.com"])
            .await
            .unwrap();

        loop {
            match executor.get_status().await {
                CommandStatus::Running => {
                    let output = executor.get_output().await;
                    if !output.is_empty() {
                        println!("Current Output:");
                        for line in output {
                            println!("{}", line);
                        }
                    }

                    let error = executor.get_error().await;
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
                CommandStatus::ErrTerminated => {
                    panic!("Built-in Command terminated with error");
                }
            }
        }
    }

    #[tokio::test]
    async fn test_customized_app_command() {
        let mut executor = CommandExecutor::new("./customized_app", &[]).await.unwrap();

        let mut all_output = Vec::new();
        loop {
            match executor.get_status().await {
                CommandStatus::Running => {
                    // collect output
                    let output = executor.get_output().await;
                    all_output.extend(output);
                    // check output error
                    let error = executor.get_error().await;
                    assert!(error.is_empty(), "Unexpected error output: {:?}", error);
                }
                CommandStatus::Finished => {
                    println!("Custom application command execution completed");
                    break;
                }
                CommandStatus::ErrTerminated => {
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

    #[tokio::test]
    async fn test_invalid_command() {
        let result = CommandExecutor::new("non_existent_command", &[]).await;
        assert!(result.is_err(), "Expected an error for invalid command");
    }
}
