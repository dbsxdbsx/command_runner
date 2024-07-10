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
        // 创建一个持续输出的命令
        let command = "ping -t 127.0.0.1";
        // 创建一个CommandExecutor实例
        let mut executor = CommandRunner::run(command).unwrap();
        // 故意调用terminate方法
        assert!(executor.terminate().is_ok());
        // 断言:
        assert_eq!(executor.get_status(), CommandStatus::ExceptionalTerminated);
    }

    #[test]
    fn test_os_command_ping() {
        let ping_count_option = if cfg!(target_os = "windows") {
            "-n"
        } else {
            "-c"
        };
        let check_num = 2;
        let executor = CommandRunner::run(&format!(
            "ping {ping_count_option} {check_num} rust-lang.org"
        ))
        .unwrap();
        let mut output_count = 0;
        loop {
            match executor.get_status() {
                CommandStatus::Running => {
                    if let Some(output) = executor.get_one_line_output() {
                        output_count += 1;
                        assert_eq!(output.get_type(), OutputType::StdOut);
                    }
                }
                CommandStatus::ExitedWithOkStatus => {
                    break;
                }
                CommandStatus::WaitingInput => {
                    panic!("There should not be `WaitingForInput` status");
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
        let executor = CommandRunner::run("python ./tests/test_output.py").unwrap();
        let mut all_output = Vec::new();
        loop {
            match executor.get_status() {
                CommandStatus::Running => {
                    // 收集输出
                    // 由于python较慢(故意延迟),所以会捕获许多`None`
                    if let Some(output) = executor.get_one_line_output() {
                        assert_eq!(output.get_type(), OutputType::StdOut);
                        all_output.push(output);
                    }
                }
                CommandStatus::ExitedWithOkStatus => {
                    println!("Custom application command execution completed");
                    break;
                }
                CommandStatus::WaitingInput => {
                    panic!("There should not be `WaitingForInput` status");
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
            assert_eq!(line.as_str(), format!("{}", i + 1))
        }
    }

    #[test]
    fn test_receiving_error_and_output_by_python_script() {
        let executor = CommandRunner::run("python ./tests/test_error.py").unwrap();
        let mut outputs = Vec::new();

        loop {
            match executor.get_status() {
                CommandStatus::Running => {
                    if let Some(output) = executor.get_one_line_output() {
                        outputs.push(output);
                    }
                }
                CommandStatus::ExitedWithOkStatus => {
                    println!("Python script execution completed");
                    break;
                }
                CommandStatus::WaitingInput => {
                    panic!("There should not be a `WaitingForInput` status");
                }
                CommandStatus::ExceptionalTerminated => {
                    panic!("Python script execution error");
                }
            }
        }

        // check outputs
        println!("the outputs are:{:?}", outputs);

        assert_eq!(outputs.len(), 4);
        assert_eq!(outputs[0].as_str(), "[1]:normal print.");
        assert_eq!(outputs[0].get_type(), OutputType::StdOut);

        assert_eq!(outputs[1].as_str(), "[2]:error print.");
        assert_eq!(outputs[1].get_type(), OutputType::StdErr);

        assert_eq!(outputs[2].as_str(), "[3]:normal print.");
        assert_eq!(outputs[2].get_type(), OutputType::StdOut);

        assert_eq!(outputs[3].as_str(), "[4]:error print.");
        assert_eq!(outputs[3].get_type(), OutputType::StdErr);
    }

    // #[test]
    // fn test_sending_input_when_command_is_inited_by_python_script() {
    //     let mut executor = CommandRunner::run("python ./tests/test_input.py").unwrap();
    //     let mut output_lines = Vec::new();
    //     let mut input_sent = false;
    //     loop {
    //         match executor.get_status() {
    //             CommandStatus::Running => {
    //                 if let Some(output) = executor.get_one_line_output() {
    //                     output_lines.push(output);
    //                 }
    //                 if let Some(error) = executor.get_one_line_error() {
    //                     panic!("测试中出现错误: {}", error);
    //                 }
    //             }
    //             CommandStatus::ExitedWithOkStatus => {
    //                 break;
    //             }
    //             CommandStatus::WaitingInput => {
    //                 if !input_sent {
    //                     executor.input("测试输入的内容").unwrap();
    //                     input_sent = true;
    //                 }
    //             }
    //             CommandStatus::ExceptionalTerminated => {
    //                 panic!();
    //             }
    //         }
    //     }
    //     assert_eq!(
    //         output_lines.len(),
    //         2,
    //         "预期输出行数为2，但实际得到 {}",
    //         output_lines.len()
    //     );
    //     assert_eq!(output_lines[0], "please input something: ");
    //     assert_eq!(
    //         output_lines[1],
    //         "you've input: 测试输入的内容. Script finished"
    //     );
    //     println!("测试通过！总输出行数: {}", output_lines.len());
    //     println!("输出内容:");
    //     for line in output_lines {
    //         println!("{}", line);
    //     }
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
    //                 let output = executor.get_one_line_output();
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

    //                 let error = executor.get_one_line_error();
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
