use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{self, JoinHandle};

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CommandStatus {
    Running,
    RunOver,
    ErrTerminated,
}

pub struct CommandExecutor {
    child: Child,
    output_receiver: Receiver<String>,
    error_receiver: Receiver<String>,
    _handles: Vec<JoinHandle<()>>,
}

impl CommandExecutor {
    pub fn new(command: &str, args: &[&str]) -> Result<Self, std::io::Error> {
        let mut child = Command::new(command)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let (output_sender, output_receiver) = channel();
        let (error_sender, error_receiver) = channel();

        let stdout_handle = Self::spawn_reader(stdout, output_sender);
        let stderr_handle = Self::spawn_reader(stderr, error_sender);

        Ok(CommandExecutor {
            child,
            output_receiver,
            error_receiver,
            _handles: vec![stdout_handle, stderr_handle],
        })
    }

    fn spawn_reader(
        stream: impl std::io::Read + Send + 'static,
        sender: Sender<String>,
    ) -> JoinHandle<()> {
        thread::spawn(move || {
            let reader = BufReader::new(stream);
            for line in reader.lines() {
                if let Ok(line) = line {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            }
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

    pub fn get_output(&self) -> Vec<String> {
        self.output_receiver.try_iter().collect()
    }

    pub fn get_error(&self) -> Vec<String> {
        self.error_receiver.try_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use std::time::Instant;

    #[test]
    fn test_command_executor() {
        let ping_count_option = if cfg!(target_os = "windows") {
            "-n"
        } else {
            "-c"
        };
        let mut executor =
            CommandExecutor::new("ping", &[ping_count_option, "2", "google.com"]).unwrap();
        let start_time = Instant::now();
        let timeout = Duration::from_secs(10);

        loop {
            match executor.get_status() {
                CommandStatus::Running => {
                    if start_time.elapsed() > timeout {
                        panic!("Command execution timed out");
                    }
                    thread::sleep(Duration::from_millis(100));

                    let output = executor.get_output();
                    if !output.is_empty() {
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
                    }
                }
                CommandStatus::RunOver => {
                    println!("Command completed successfully");
                    break;
                }
                CommandStatus::ErrTerminated => {
                    panic!("Command terminated with error");
                }
            }
        }
    }
}
