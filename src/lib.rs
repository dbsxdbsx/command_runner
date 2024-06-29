use anyhow::{Context, Result};
use encoding_rs::GB18030;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CommandStatus {
    Running,
    RunOver,
    ErrTerminated,
}

pub struct CommandRunner {
    child: std::process::Child,
    output: Arc<Mutex<Vec<String>>>,
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

        Self::spawn_reader_thread(BufReader::new(stdout), Arc::clone(&output));
        Self::spawn_error_thread(BufReader::new(stderr), Arc::clone(&output), error_tx);

        Ok(CommandRunner {
            child,
            output,
            error_rx,
        })
    }

    fn spawn_reader_thread<R: 'static + Send + BufRead>(
        reader: R,
        output: Arc<Mutex<Vec<String>>>,
    ) {
        thread::spawn(move || {
            let mut reader = reader;
            let mut buffer = Vec::new();
            loop {
                buffer.clear();
                match reader.read_until(b'\n', &mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let (decoded, _, _) = GB18030.decode(&buffer);
                        let line = decoded.trim_end().to_string();
                        if !line.is_empty() {
                            let mut output = output.lock().unwrap();
                            output.push(line);
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    fn spawn_error_thread<R: 'static + Send + BufRead>(
        reader: R,
        output: Arc<Mutex<Vec<String>>>,
        error_tx: mpsc::Sender<String>,
    ) {
        thread::spawn(move || {
            let mut reader = reader;
            let mut buffer = Vec::new();
            loop {
                buffer.clear();
                match reader.read_until(b'\n', &mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let (decoded, _, _) = GB18030.decode(&buffer);
                        let line = decoded.trim_end().to_string();
                        if !line.is_empty() {
                            let mut output = output.lock().unwrap();
                            output.push(line.clone());
                            let _ = error_tx.send(line);
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
        self.child.wait().context("Failed to wait for child process")?;
        Ok(CommandStatus::RunOver)
    }

    pub fn provide_input(&mut self, input: &str) -> Result<()> {
        if let Some(stdin) = &mut self.child.stdin {
            stdin.write_all(input.as_bytes()).context("Failed to write to stdin")?;
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
        let mut runner = CommandRunner::run("sleep 2").expect("Failed to create CommandRunner");
        assert_eq!(runner.get_status(), CommandStatus::Running);
        thread::sleep(Duration::from_secs(2));
        assert_eq!(runner.get_status(), CommandStatus::RunOver);
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
                println!("Got terminal Output:\n{}", output);
                assert!(!output.trim().is_empty());
                output_count += 1;
            }
            let status = runner.get_status();
            assert!(status != CommandStatus::ErrTerminated);
            if status == CommandStatus::RunOver {
                break;
            }
            thread::sleep(Duration::from_millis(500));
        }
        assert!(output_count >= ping_num, "Only received {output_count} outputs");
        assert_eq!(runner.get_status(), CommandStatus::RunOver);
    }
}
