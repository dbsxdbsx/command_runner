use anyhow::{Context, Result};
use encoding_rs::GB18030;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub trait OutputExt {
    fn to_str(&self) -> Result<String>;
}
impl OutputExt for Vec<u8> {
    fn to_str(&self) -> Result<String> {
        let (decoded, _, _) = GB18030.decode(self);
        Ok(decoded.to_string())
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CommandStatus {
    Running,
    RunOver,
    ErrTerminated,
}

pub struct CommandRunner {
    child: std::process::Child,
    output: Arc<Mutex<Vec<u8>>>,
    error_rx: mpsc::Receiver<String>,
}

impl CommandRunner {
    pub fn run(command: &str) -> Result<Self> {
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
            .spawn()
            .context("Failed to spawn command")?;

        let output = Arc::new(Mutex::new(Vec::new()));
        let stdout = child.stdout.take().expect("Failed to capture stdout");
        let stderr = child.stderr.take().expect("Failed to capture stderr");
        let (error_tx, error_rx) = mpsc::channel();

        Self::spawn_io_thread(
            BufReader::new(stdout),
            BufReader::new(stderr),
            Arc::clone(&output),
            error_tx,
        );

        Ok(CommandRunner {
            child,
            output,
            error_rx,
        })
    }

    fn spawn_io_thread<STDOUT: 'static + Send + BufRead, STDERR: 'static + Send + BufRead>(
        mut stdout_reader: STDOUT,
        mut stderr_reader: STDERR,
        output: Arc<Mutex<Vec<u8>>>,
        error_tx: mpsc::Sender<String>,
    ) {
        thread::spawn(move || {
            let mut stdout_buffer = Vec::new();
            let mut stderr_buffer = Vec::new();

            loop {
                stdout_buffer.clear();
                stderr_buffer.clear();

                let stdout_result = stdout_reader.read_until(b'\n', &mut stdout_buffer);
                if let Ok(bytes_read) = stdout_result {
                    if bytes_read > 0 {
                        let mut output = output.lock().unwrap();
                        output.extend_from_slice(&stdout_buffer);
                    }
                }

                let stderr_result = stderr_reader.read_until(b'\n', &mut stderr_buffer);
                if let Ok(bytes_read) = stderr_result {
                    if bytes_read > 0 {
                        let mut output = output.lock().unwrap();
                        output.extend_from_slice(&stderr_buffer);
                        let line = String::from_utf8_lossy(&stderr_buffer).to_string();
                        let _ = error_tx.send(line);
                    }
                }

                if stdout_result.is_err() && stderr_result.is_err() {
                    break;
                }
            }
        });
    }

    pub fn get_output(&self) -> Option<Vec<u8>> {
        let mut output = self.output.lock().unwrap();
        if !output.is_empty() {
            Some(output.split_off(0))
        } else {
            None
        }
    }

    pub fn get_status(&mut self) -> CommandStatus {
        match self.child.try_wait() {
            Ok(Some(_)) => CommandStatus::RunOver,
            Ok(None) => {
                thread::sleep(Duration::from_millis(100));
                if let Ok(error) = self.error_rx.try_recv() {
                    eprintln!("Command error: {}", error);
                    CommandStatus::ErrTerminated
                } else {
                    CommandStatus::Running
                }
            }
            Err(e) => {
                panic!("Failed to wait for child process: {}", e);
            }
        }
    }

    pub fn terminate(&mut self) -> Result<CommandStatus> {
        self.child.kill().context("Failed to kill child process")?;
        self.child
            .wait()
            .context("Failed to wait for child process")?;
        Ok(CommandStatus::RunOver)
    }

    pub fn provide_input(&mut self, input: &str) -> Result<()> {
        if let Some(stdin) = &mut self.child.stdin {
            stdin
                .write_all(input.as_bytes())
                .context("Failed to write to stdin")?;
            stdin.flush().context("Failed to flush stdin")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_valid_command() {
        let mut result = CommandRunner::run("echo").unwrap();
        assert_eq!(result.get_status(), CommandStatus::Running);
    }

    #[test]
    fn test_invalid_command() {
        let mut result = CommandRunner::run("invalid_command").unwrap();
        assert_eq!(result.get_status(), CommandStatus::ErrTerminated);
    }

    #[test]
    fn test_command_feedback() {
        let ping_count_option = if cfg!(target_os = "windows") {
            "-n"
        } else {
            "-c"
        };
        let ping_num = 2;
        let ping_command = format!("ping {} {} google.com", ping_count_option, ping_num);
        let mut runner = CommandRunner::run(&ping_command).expect("Failed to create CommandRunner");
        let mut output_count = 0;
        loop {
            if let Some(output) = runner.get_output() {
                let output_str = output.to_str().expect("Failed to convert output to string");
                println!("Got terminal Output:\n{}", output_str);
                assert!(!output_str.trim().is_empty());
                output_count += 1;
            }
            let status = runner.get_status();
            assert!(status != CommandStatus::ErrTerminated);
            if status == CommandStatus::RunOver {
                break;
            }
            thread::sleep(Duration::from_millis(500));
        }
        assert!(
            output_count >= ping_num,
            "Only received {output_count} outputs"
        );
        assert_eq!(runner.get_status(), CommandStatus::RunOver);
    }
}
