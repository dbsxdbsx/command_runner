use encoding_rs::GB18030;
use std::io::{self, BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use which::which;

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
    pub fn new(command: &str, max_output_size: usize) -> io::Result<Self> {
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

        let stdout = child.stdout.take().expect("Failed to capture stdout");
        let stderr = child.stderr.take().expect("Failed to capture stderr");

        // 创建一个用于存储stderr输出的缓冲区
        let stderr_buffer = Arc::new(Mutex::new(Vec::new()));

        Self::spawn_reader_thread(
            stdout,
            Arc::clone(&output),
            Arc::clone(&running),
            max_output_size,
        );
        Self::spawn_reader_thread(
            stderr,
            Arc::clone(&stderr_buffer),
            Arc::clone(&running),
            max_output_size,
        );

        // 等待一小段时间，让命令有机会产生一些输出
        thread::sleep(Duration::from_millis(100));

        // 检查stderr是否有输出
        let stderr_output = stderr_buffer.lock().unwrap();
        if !stderr_output.is_empty() {
            let error_message = stderr_output.join("\n");
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Command produced error output: {}", error_message),
            ));
        }

        Ok(CommandRunner {
            child: Some(child),
            output,
            running,
        })
    }

    fn spawn_reader_thread<R: io::Read + Send + 'static>(
        reader: R,
        output: Arc<Mutex<Vec<String>>>,
        running: Arc<std::sync::atomic::AtomicBool>,
        max_output_size: usize,
    ) {
        thread::spawn(move || {
            let mut reader = BufReader::new(reader);
            let mut buffer = Vec::new();
            loop {
                if !running.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }

                buffer.clear();
                match reader.read_until(b'\n', &mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let (decoded, _, _) = GB18030.decode(&buffer);
                        let line = decoded.trim_end().to_string();
                        if !line.is_empty() {
                            let mut output = output.lock().unwrap();
                            if output.len() < max_output_size {
                                output.push(line);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
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

    pub fn terminate(&mut self) -> io::Result<CommandStatus> {
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

    pub fn provide_input(&mut self, input: &str) -> io::Result<()> {
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
                assert!(!output.trim().is_empty());
                output_count += 1;
            }
            let status = runner.get_status();
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
        // valid command
        let result = CommandRunner::new("echo", 10000);
        assert!(result.is_ok());

        // err command
        let result = CommandRunner::new("nonexistent_command", 10000);
        assert!(result.is_err());
    }
}
