#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn test_invalid_command() {
        let result = CommandRunner::run("non_existent_command");
        assert!(result.is_err(), "Expected an error for invalid command");
    }

    #[test]
    fn test_force_terminate_by_os_command_ping() {
        // Create a command that outputs continuously
        let command = "ping -t 127.0.0.1";

        // Create a CommandExecutor instance
        let mut executor = CommandRunner::run(command).unwrap();

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

        // Assertions:
        let status = executor.get_status();
        assert!(
            matches!(status, CommandStatus::ExceptionalTerminated),
            "The process should have terminated"
        );
    }

    #[test]
    fn test_os_command_ping() {
        let ping_count_option = if cfg!(target_os = "windows") {
            "-n"
        } else {
            "-c"
        };

        let check_num = 2;
        let mut executor =
            CommandRunner::run(&format!("ping {ping_count_option} {check_num} google.com"))
                .unwrap();

        let mut output_count = 0;
        loop {
            match executor.get_status() {
                CommandStatus::Inited => {
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
                CommandStatus::Exited => {
                    println!("Built-in Command completed successfully");
                    break;
                }
                CommandStatus::WaitInput => {
                    panic!("There should not be `WaitingForInput` status")
                }
                CommandStatus::ExceptionalTerminated => {
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
    fn test_receiving_output_by_python_script() {
        let mut executor = CommandRunner::run("python ./tests/test_output.py").unwrap();

        let mut all_output = Vec::new();
        loop {
            match executor.get_status() {
                CommandStatus::Inited => {
                    // collect output
                    let output = executor.get_output();
                    all_output.extend(output);
                    // check output error
                    let error = executor.get_error();
                    assert!(error.is_empty(), "Unexpected error output: {:?}", error);
                }
                CommandStatus::Exited => {
                    println!("Custom application command execution completed");
                    break;
                }
                CommandStatus::WaitInput => {
                    panic!("There should not be `WaitingForInput` status")
                }
                CommandStatus::ExceptionalTerminated => {
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

        for (i, line) in all_output.iter().enumerate() {
            assert_eq!(
                line.trim(),
                &(i + 1).to_string(),
                "Line {} should be '{}'",
                i + 1,
                i + 1
            );
        }
    }

    // #[test]
    // fn test_sending_input_when_command_is_inited_by_python_script() {
    //     let mut executor = CommandRunner::run("python ./tests/test_input.py").unwrap();

    //     let mut output_count = 0;
    //     let mut input_sent = false;

    //     loop {
    //         match executor.get_status() {
    //             CommandStatus::Inited => {
    //                 let output = executor.get_output();
    //                 if !output.is_empty() {
    //                     output_count += output.len();
    //                     println!("当前输出:");
    //                     for line in output {
    //                         println!("{}", line);
    //                     }
    //                 }

    //                 let error = executor.get_error();
    //                 if !error.is_empty() {
    //                     println!("当前错误:");
    //                     for line in error {
    //                         println!("{}", line);
    //                     }
    //                     panic!("此测试用例中不应出现错误!");
    //                 }
    //             }
    //             CommandStatus::Exited => {
    //                 break;
    //             }
    //             CommandStatus::WaitInput => {
    //                 if !input_sent {
    //                     executor.input("测试输入的内容").unwrap();
    //                     input_sent = true;
    //                 }
    //             }
    //             CommandStatus::ExceptionalTerminated => {
    //                 panic!("Python脚本异常终止");
    //             }
    //         }
    //     }

    //     assert_eq!(
    //         output_count, 3,
    //         "预期输出行数为3,但实际得到{}",
    //         output_count
    //     );
    //     println!("总输出行数: {}", output_count);
    // }

    // #[test]
    // fn test_input_and_output_by_python_script_guessing_game() {
    //     let mut executor = CommandRunner::run("python ./tests/guessing_game.py").unwrap();

    //     let mut all_output = Vec::new();
    //     let mut min = 1;
    //     let mut max = 100;
    //     let mut guess = 50;

    //     loop {
    //         match executor.get_status() {
    //             CommandStatus::Running => {
    //                 let output = executor.get_output();
    //                 println!("the output is:{output:?}");
    //                 all_output.extend(output.clone());

    //                 for line in output {
    //                     println!("Output: {}", line);
    //                     if line.contains("Too small!") {
    //                         min = guess + 1;
    //                     } else if line.contains("Too big!") {
    //                         max = guess - 1;
    //                     } else if line.contains("You win!") {
    //                         println!("游戏胜利!");
    //                         return;
    //                     }
    //                 }

    //                 let error = executor.get_error();
    //                 assert!(error.is_empty(), "意外的错误输出: {:?}", error);
    //             }
    //             CommandStatus::WaitingForInput => {
    //                 guess = (min + max) / 2;
    //                 executor.input(&guess.to_string()).unwrap();
    //                 println!("输入: {}", guess);
    //             }
    //             CommandStatus::Finished => {
    //                 panic!("游戏意外结束,没有胜利");
    //             }
    //             CommandStatus::ExceptionTerminated => {
    //                 panic!("游戏异常终止");
    //             }
    //         }
    //     }
    // }
}
