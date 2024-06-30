use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::mpsc;

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CommandStatus {
    Running,
    Finished,
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
        mut stream: impl tokio::io::AsyncRead + Unpin,
        sender: mpsc::UnboundedSender<String>,
    ) {
        let mut buffer = Vec::new();
        let mut temp_buffer = [0u8; 1024];

        loop {
            match stream.read(&mut temp_buffer).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    buffer.extend_from_slice(&temp_buffer[..n]);
                    while let Some(i) = buffer.iter().position(|&x| x == b'\n') {
                        let line = String::from_utf8_lossy(&buffer[..i]).to_string();
                        if sender.send(line).is_err() {
                            return;
                        }
                        buffer = buffer.split_off(i + 1);
                    }
                }
                Err(_) => break,
            }
        }

        // 发送剩余的缓冲区内容(如果有的话)
        if !buffer.is_empty() {
            let line = String::from_utf8_lossy(&buffer).to_string();
            let _ = sender.send(line);
        }
    }

    pub async fn get_status(&mut self) -> CommandStatus {
        match self.child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    CommandStatus::Finished
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

    #[tokio::test]
    async fn test_os_built_in_command() {
        let ping_count_option = if cfg!(target_os = "windows") {
            "-n"
        } else {
            "-c"
        };

        let mut executor = CommandExecutor::new("ping", &[ping_count_option, "1", "google.com"])
            .await
            .unwrap();

        loop {
            match executor.get_status().await {
                CommandStatus::Running => {
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
                CommandStatus::Finished => {
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
