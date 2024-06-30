mod test;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
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
    stream_tasks: Vec<tokio::task::JoinHandle<()>>,
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

        let stdout_task = tokio::spawn(Self::read_stream(stdout, output_sender));
        let stderr_task = tokio::spawn(Self::read_stream(stderr, error_sender));

        Ok(CommandExecutor {
            child,
            output_receiver,
            error_receiver,
            stream_tasks: vec![stdout_task, stderr_task],
        })
    }

    async fn read_stream(
        stream: impl tokio::io::AsyncRead + Unpin,
        sender: mpsc::UnboundedSender<String>,
    ) {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();

        loop {
            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    if sender.send(line.trim().to_string()).is_err() {
                        break;
                    }
                    line.clear();
                }
                Err(_) => break,
            }
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
