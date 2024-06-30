use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CommandStatus {
    Running,
    RunOver,
    ErrTerminated,
}

pub struct CommandExecutor {
    child: tokio::process::Child,
    output_receiver: mpsc::UnboundedReceiver<String>,
    error_receiver: mpsc::UnboundedReceiver<String>,
}

impl CommandExecutor {
    pub async fn new(command: &str, args: &[&str]) -> Result<Self, std::io::Error> {
        let (output_sender, output_receiver) = mpsc::unbounded_channel();
        let (error_sender, error_receiver) = mpsc::unbounded_channel();

        let mut child = Command::new(command)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().expect("Failed to capture stdout");
        let stderr = child.stderr.take().expect("Failed to capture stderr");

        tokio::spawn(Self::read_stream(stdout, output_sender));
        tokio::spawn(Self::read_stream(stderr, error_sender));

        Ok(CommandExecutor {
            child,
            output_receiver,
            error_receiver,
        })
    }

    async fn read_stream(
        stream: impl tokio::io::AsyncRead + Unpin,
        sender: mpsc::UnboundedSender<String>,
    ) {
        let mut reader = BufReader::new(stream).lines();

        while let Some(line) = reader.next_line().await.unwrap() {
            if sender.send(line).is_err() {
                break;
            }
        }
    }

    pub async fn get_status(&mut self) -> CommandStatus {
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

    pub async fn get_output(&mut self) -> Vec<String> {
        let mut output = Vec::new();
        while let Ok(line) = self.output_receiver.try_recv() {
            output.push(line);
        }
        output
    }

    pub async fn get_error(&mut self) -> Vec<String> {
        let mut error = Vec::new();
        while let Ok(line) = self.error_receiver.try_recv() {
            error.push(line);
        }
        error
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use std::time::Instant;

    #[tokio::test]
    async fn test_command_executor() {
        let ping_count_option = if cfg!(target_os = "windows") {
            "-n"
        } else {
            "-c"
        };

        let mut executor = CommandExecutor::new("ping", &[ping_count_option, "2", "google.com"])
            .await
            .unwrap();

        let start_time = Instant::now();
        let timeout = Duration::from_secs(2);
        loop {
            match executor.get_status().await {
                CommandStatus::Running => {
                    if start_time.elapsed() > timeout {
                        panic!("Command execution timed out");
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;

                    let output = executor.get_output().await;
                    if !output.is_empty() {
                        println!("Current Output:");
                        for line in output {
                            println!("{}", line);
                        }
                    }

                    let error = executor.get_error().await;
                    if !error.is_empty() {
                        println!("Current Error:");
                        for line in error {
                            println!("{}", line);
                        }
                        panic!("There should not be error in this test case!")
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

    #[tokio::test]
    async fn test_invalid_command() {
        let result = CommandExecutor::new("non_existent_command", &[]).await;
        assert!(result.is_err(), "Expected an error for invalid command");
    }
}
