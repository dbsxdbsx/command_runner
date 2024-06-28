use anyhow::{Context, Result};
use std::io::{self, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CommandStatus {
    Running,
    Terminated,
}

pub struct CommandRunner {
    output: Arc<Mutex<Vec<String>>>,
    running: Arc<AtomicBool>,
    child: Option<Child>,
    max_output_size: usize,
}

impl CommandRunner {
    pub fn new(cmd: &str, max_output_size: usize) -> Result<Self> {
        let mut runner = CommandRunner {
            output: Arc::new(Mutex::new(Vec::new())),
            running: Arc::new(AtomicBool::new(false)),
            child: None,
            max_output_size,
        };
        runner.execute(cmd)?;
        Ok(runner)
    }

    fn execute(&mut self, cmd: &str) -> Result<()> {
        let mut command = if cfg!(target_os = "windows") {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(cmd);
            c
        } else {
            let mut c = Command::new("sh");
            c.arg("-c").arg(cmd);
            c
        };

        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.stdin(Stdio::piped());

        let mut child = command.spawn().context("Failed to spawn command")?;

        // 等待一小段时间，让进程有机会启动并可能失败
        thread::sleep(Duration::from_millis(100));
        // 检查进程是否已经退出
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Err(anyhow::anyhow!("Command failed with exit code: {}", status));
                }
            }
            Ok(None) => {} // 进程仍在运行，这是正常的
            Err(e) => return Err(e.into()),
        }

        self.child = Some(child);

        let output = Arc::clone(&self.output);
        let running = Arc::clone(&self.running);
        running.store(true, Ordering::SeqCst);

        let mut stdout = self
            .child
            .as_mut()
            .unwrap()
            .stdout
            .take()
            .expect("Failed to open stdout");
        let mut stderr = self
            .child
            .as_mut()
            .unwrap()
            .stderr
            .take()
            .expect("Failed to open stderr");

        let running_clone = Arc::clone(&running);
        let max_output_size = self.max_output_size;
        thread::spawn(move || {
            let mut buffer = [0; 128];
            while running_clone.load(Ordering::SeqCst) {
                match stdout.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let mut output = output.lock().unwrap();
                        if output.len() < max_output_size {
                            output.push(String::from_utf8_lossy(&buffer[..n]).to_string());
                        }
                    }
                }
            }
        });

        let output = Arc::clone(&self.output);
        let running_clone = Arc::clone(&running);
        thread::spawn(move || {
            let mut buffer = [0; 128];
            while running_clone.load(Ordering::SeqCst) {
                match stderr.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let mut output = output.lock().unwrap();
                        if output.len() < max_output_size {
                            output.push(String::from_utf8_lossy(&buffer[..n]).to_string());
                        }
                    }
                }
            }
        });

        Ok(())
    }

    pub fn get_output(&self) -> Option<String> {
        let mut output = self.output.lock().unwrap();
        output.pop()
    }

    pub fn get_status(&mut self) -> CommandStatus {
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(Some(_)) => {
                    self.running.store(false, Ordering::SeqCst);
                    CommandStatus::Terminated
                }
                Ok(None) => CommandStatus::Running,
                Err(_) => {
                    self.running.store(false, Ordering::SeqCst);
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
            self.running.store(false, Ordering::SeqCst);
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
                println!("Got Output: {}", output);
                output_count += 1;
            }
            let status = runner.get_status();
            println!("Current status: {:?}", status);
            if status == CommandStatus::Terminated {
                break;
            }
            thread::sleep(Duration::from_millis(500));
        }

        assert!(output_count >= ping_num, "Only received {output_count} outputs");
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
