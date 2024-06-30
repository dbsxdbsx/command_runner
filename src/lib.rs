use std::io::{self, BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CommandStatus {
    Running,
    RunOver,
    ErrTerminated,
}

/// CommandExecutor 结构体，用于执行命令行指令并处理输出和输入
pub struct CommandExecutor {
    output: Arc<Mutex<String>>,
    sender: Sender<String>,
}

impl Default for CommandExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandExecutor {
    /// 创建一个新的 CommandExecutor 实例
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<String>();
        let output = Arc::new(Mutex::new(String::new()));

        // 启动一个线程来处理输出
        let output_clone = Arc::clone(&output);
        thread::spawn(move || {
            while let Ok(line) = receiver.recv() {
                let mut output = output_clone.lock().unwrap();
                output.push_str(&line);
                output.push('\n');
            }
        });

        CommandExecutor { output, sender }
    }

    /// 执行命令行指令
    pub fn execute_command(&self, command: &str, args: &[&str]) -> io::Result<()> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let mut stdin = child.stdin.take().unwrap();

        let sender_clone = self.sender.clone();

        // 处理标准输出
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if let Ok(line) = line {
                    sender_clone.send(line).unwrap();
                }
            }
        });

        // 处理标准错误
        let sender_clone = self.sender.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(line) = line {
                    sender_clone.send(line).unwrap();
                }
            }
        });

        // 处理用户输入
        thread::spawn(move || {
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            stdin.write_all(input.as_bytes()).unwrap();
        });

        Ok(())
    }

    /// 获取当前的输出
    pub fn get_output(&self) -> String {
        let output = self.output.lock().unwrap();
        output.clone()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_basic() {
        let executor = CommandExecutor::new();
        let command = "ping";
        let args = ["-c", "2", "google.com"];

        match executor.execute_command(command, &args) {
            Ok(_) => {
                let mut output = String::new();
                let start_time = std::time::Instant::now();
                let timeout = Duration::from_secs(10); // 设置10秒超时

                while start_time.elapsed() < timeout {
                    let current_output = executor.get_output();
                    if current_output != output {
                        output = current_output;
                        println!("Current Output:\n{}", output);
                    }

                    match executor.get_status() {
                        CommandStatus::RunOver => break,
                        CommandStatus::ErrTerminated => {
                            panic!("Command terminated with error");
                        }
                        CommandStatus::Running => {
                            std::thread::sleep(Duration::from_millis(100));
                        }
                    }
                }

                assert!(!output.is_empty(), "Command output should not be empty");
                assert_eq!(executor.get_status(), CommandStatus::RunOver, "Command should be completed");
            }
            Err(e) => {
                eprintln!("Failed to execute command: {}", e);
                panic!("Command execution failed");
            }
        }
    }
}