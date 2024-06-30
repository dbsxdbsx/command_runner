use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CommandStatus {
    Running,
    RunOver,
    ErrTerminated,
}

pub struct CommandExecutor {
    child: Child,
    output: Arc<Mutex<String>>,
    error: Arc<Mutex<String>>,
}

impl CommandExecutor {
    pub fn new(command: &str, args: &[&str]) -> Result<Self, std::io::Error> {
        let (output_tx, output_rx) = channel();
        let (error_tx, error_rx) = channel();
        let output = Arc::new(Mutex::new(String::new()));
        let error = Arc::new(Mutex::new(String::new()));

        let mut child = Command::new(command)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        // 处理 stdout
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if let Ok(line) = line {
                    output_tx.send(line).unwrap();
                }
            }
        });

        // 处理 stderr
        let error_tx_clone = error_tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(line) = line {
                    error_tx_clone.send(line).unwrap();
                }
            }
        });

        // 在主线程中处理输出和错误
        let output_clone = Arc::clone(&output);
        let error_clone = Arc::clone(&error);
        thread::spawn(move || {
            while let Ok(line) = output_rx.recv() {
                let mut output = output_clone.lock().unwrap();
                output.push_str(&line);
                output.push('\n');
            }
        });
        thread::spawn(move || {
            while let Ok(line) = error_rx.recv() {
                let mut error = error_clone.lock().unwrap();
                error.push_str(&line);
                error.push('\n');
            }
        });

        Ok(CommandExecutor {
            child,
            output,
            error,
        })
    }

    pub fn get_status(&mut self) -> CommandStatus {
        match self.child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    CommandStatus::RunOver
                } else {
                    CommandStatus::ErrTerminated
                }
            }
            Ok(None) => CommandStatus::Running,
            Err(e) => {
                eprintln!("Failed to wait for child process: {}", e);
                CommandStatus::ErrTerminated
            }
        }
    }

    pub fn get_output(&self) -> String {
        let output = self.output.lock().unwrap();
        output.clone()
    }

    pub fn get_error(&self) -> String {
        let error = self.error.lock().unwrap();
        error.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_command_executor() {
        let mut executor = CommandExecutor::new("ping", &["-c", "3", "google.com"]).unwrap();
        let start_time = Instant::now();
        let timeout = Duration::from_secs(10);

        loop {
            match executor.get_status() {
                CommandStatus::Running => {
                    if start_time.elapsed() > timeout {
                        panic!("Command execution timed out");
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                CommandStatus::RunOver => {
                    println!("Command completed successfully");
                    break;
                }
                CommandStatus::ErrTerminated => {
                    panic!("Command terminated with error");
                }
            }

            // 打印新的输出
            while let Ok(line) = executor.output_rx.try_recv() {
                println!("Output: {}", line);
            }

            // 打印新的错误
            while let Ok(line) = executor.error_rx.try_recv() {
                eprintln!("Error: {}", line);
            }
        }

        let output = executor.get_output();
        let error = executor.get_error();

        assert!(!output.is_empty(), "Output should not be empty");
        assert!(
            output.contains("3 packets transmitted"),
            "Output should contain expected content"
        );
        assert!(
            error.is_empty(),
            "Error should be empty for successful command"
        );
    }
}
