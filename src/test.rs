#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    #[should_panic(expected = "Child process is not initialized yet.")]
    fn test_check_is_running_when_not_initialized() {
        let executor = CommandRunner::new("ping -t 127.0.0.1").unwrap();
        assert!(executor.is_running());
    }

    #[test]
    #[should_panic(expected = "Child process is not initialized yet.")]
    fn test_check_is_stopped_when_not_initialized() {
        let executor = CommandRunner::new("ping -t 127.0.0.1").unwrap();
        assert!(executor.is_stopped());
    }

    #[test]
    fn test_status_for_no_ending_command() {
        let command = "ping -t 127.0.0.1";
        let mut executor = CommandRunner::new(command).unwrap();
        // run the command
        executor.run();
        assert!(executor.is_running());
        // stop the command
        executor.stop();
        assert!(executor.is_stopped());
    }

    #[test]
    fn test_status_for_auto_ended_command() {
        let ping_count_option = if cfg!(target_os = "windows") {
            "-n"
        } else {
            "-c"
        };
        let check_num = 1;
        let mut executor = CommandRunner::new(&format!(
            "ping {ping_count_option} {check_num} rust-lang.org"
        ))
        .unwrap();
        executor.run();
        // the instant status of the command should be Running
        assert!(executor.is_running());
        // wait for auto-termination of the command, when the status should be Stopped
        std::thread::sleep(std::time::Duration::from_secs(1));
        assert!(executor.is_stopped());
    }

    #[test]
    fn test_invalid_command() {
        let result = CommandRunner::new("non_existent_command");
        assert!(result.is_err(), "Expected an error for invalid command");
    }

    #[test]
    fn test_std_output_from_os_command() {
        let ping_count_option = if cfg!(target_os = "windows") {
            "-n"
        } else {
            "-c"
        };
        let check_num = 2;
        let mut executor = CommandRunner::new(&format!(
            "ping {ping_count_option} {check_num} rust-lang.org"
        ))
        .unwrap();
        executor.run();
        let mut output_count = 0;

        while executor.is_running() {
            if let Some(output) = executor.get_one_line_output() {
                output_count += 1;
                assert_eq!(output.get_type(), OutputType::StdOut);
            }
        }
        assert!(
            output_count > check_num,
            "Expected output count to be greater than {}, but got {}",
            check_num,
            output_count
        );
    }

    #[test]
    fn test_std_output_from_python_script() {
        let mut executor = CommandRunner::new("python ./tests/test_output.py").unwrap();
        executor.run();
        let mut all_output = Vec::new();
        while executor.is_running() {
            if let Some(output) = executor.get_one_line_output() {
                assert_eq!(output.get_type(), OutputType::StdOut);
                all_output.push(output);
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
    fn test_std_output_and_error_from_python_script() {
        let mut executor = CommandRunner::new("python ./tests/test_error.py").unwrap();
        executor.run();

        let mut outputs = Vec::new();
        while executor.is_running() {
            if let Some(output) = executor.get_one_line_output() {
                outputs.push(output);
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
