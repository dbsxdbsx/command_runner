use encoding_rs::{Encoding, UTF_8};
use std::io::{self, Read, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CommandStatus {
    Running,
    Terminated,
}

pub struct CommandRunner {
    child: Option<std::process::Child>,
    output: Arc<Mutex<Vec<String>>>,
    running: Arc<std::sync::atomic::AtomicBool>,
}

impl CommandRunner {
    pub fn new(command: &str, max_output_size: usize) -> Result<Self, io::Error> {
        let (cmd_exe, param1) = if cfg!(target_os = "windows") {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };

        let mut child = Command::new(cmd_exe)
            .arg(param1)
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::piped())
            .spawn()?;

        let output = Arc::new(Mutex::new(Vec::new()));
        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));

        thread::sleep(Duration::from_millis(100));

        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Command failed with exit code: {}", status),
                    ));
                }
            }
            Ok(None) => {} // Process is still running, this is normal
            Err(e) => return Err(e.into()),
        }

        let output_clone = Arc::clone(&output);
        let running_clone = Arc::clone(&running);
        let stdout = child.stdout.take();
        thread::spawn(move || {
            let mut buffer = [0; 128];
            if let Some(mut stdout) = stdout {
                while running_clone.load(std::sync::atomic::Ordering::SeqCst) {
                    match stdout.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let mut output = output_clone.lock().unwrap();
                            if output.len() < max_output_size {
                                let (decoded, _) = UTF_8.decode_without_bom_handling(&buffer[..n]);
                                output.push(decoded.into_owned());
                            }
                        }
                    }
                }
            }
        });

        let output_clone = Arc::clone(&output);
        let running_clone = Arc::clone(&running);
        let stderr = child.stderr.take();
        thread::spawn(move || {
            let mut buffer = [0; 128];
            if let Some(mut stderr) = stderr {
                while running_clone.load(std::sync::atomic::Ordering::SeqCst) {
                    match stderr.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let mut output = output_clone.lock().unwrap();
                            if output.len() < max_output_size {
                                let (decoded, _) = UTF_8.decode_without_bom_handling(&buffer[..n]);
                                output.push(decoded.into_owned());
                            }
                        }
                    }
                }
            }
        });

        Ok(CommandRunner {
            child: Some(child),
            output,
            running,
        })
    }

    pub fn get_output(&self) -> Option<String> {
        let mut output = self.output.lock().unwrap();
        output.pop()
    }

    pub fn get_status(&mut self) -> CommandStatus {
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(Some(_)) => {
                    self.running
                        .store(false, std::sync::atomic::Ordering::SeqCst);
                    CommandStatus::Terminated
                }
                Ok(None) => CommandStatus::Running,
                Err(_) => {
                    self.running
                        .store(false, std::sync::atomic::Ordering::SeqCst);
                    CommandStatus::Terminated
                }
            }
        } else {
            CommandStatus::Terminated
        }
    }

    pub fn terminate(&mut self) -> Result<CommandStatus, io::Error> {
        if let Some(child) = &mut self.child {
            child.kill()?;
            self.running
                .store(false, std::sync::atomic::Ordering::SeqCst);
            child.wait()?;
            Ok(CommandStatus::Terminated)
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "No running command to terminate",
            ))
        }
    }

    pub fn provide_input(&mut self, input: &str) -> Result<(), io::Error> {
        if let Some(child) = &mut self.child {
            if let Some(stdin) = &mut child.stdin {
                stdin.write_all(input.as_bytes())?;
                stdin.flush()?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    #[test]
    fn test_successful_command() {
        let ping_count_option = if cfg!(target_os = "windows") {
            "-n"
        } else {
            "-c"
        };
        let ping_num = 2;
        let ping_command = format!("ping {} {} google.com", ping_count_option, ping_num);
        let mut runner =
            CommandRunner::new(&ping_command, 10000).expect("Failed to create CommandRunner");

        let mut output_count = 0;
        loop {
            if let Some(output) = runner.get_output() {
                println!("Got terminal Output:\n{}", output);
                output_count += 1;
            }
            let status = runner.get_status();
            println!("Got terminal status: {:?}", status);
            if status == CommandStatus::Terminated {
                break;
            }
            thread::sleep(Duration::from_millis(500));
        }

        assert!(
            output_count >= ping_num,
            "Only received {output_count} outputs"
        );
        assert_eq!(runner.get_status(), CommandStatus::Terminated);
    }

    #[test]
    fn test_panic_command() {
        let result = CommandRunner::new("nonexistent_command", 10000);
        assert!(result.is_err());
    }

    // TODO:
    // #[test]
    // fn test_provide_input() {
    //     // 假设 guessing_game 项目已经编译为可执行文件
    //     let app = "python";
    //     let mut runner =
    //         CommandRunner::new(app, 10000).expect("Failed to create CommandRunner");

    //     // 检查初始反馈是否为 `>>>`
    //     // let mut initial_feedback_received = false;
    //     // loop {
    //     //     if let Some(output) = runner.get_output() {
    //     //         println!("Got Output: {}", output);
    //     //         if output.trim() == ">>>" {
    //     //             initial_feedback_received = true;
    //     //             break;
    //     //         }
    //     //     }
    //     //     let status = runner.get_status();
    //     //     println!("Current status: {:?}", status);
    //     //     if status == CommandStatus::Terminated {
    //     //         break;
    //     //     }
    //     //     thread::sleep(Duration::from_millis(500));
    //     // }

    //     // assert!(initial_feedback_received, "Initial feedback not received");

    //     // 输入 `exit()`
    //     runner
    //         .provide_input("exit()\n")
    //         .expect("Failed to provide input");

    //     // 检查命令是否终止
    //     loop {
    //         let status = runner.get_status();
    //         println!("Current status: {:?}", status);
    //         if status == CommandStatus::Terminated {
    //             break;
    //         }
    //         thread::sleep(Duration::from_millis(500));
    //     }

    //     assert_eq!(runner.get_status(), CommandStatus::Terminated);
    // }
}
