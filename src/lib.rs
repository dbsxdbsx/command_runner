mod test;

use anyhow::{Context, Result};
use encoding_rs::GB18030;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::Notify;

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CommandStatus {
    Running,               // the command is valid initialized and is running
    ExitedWithOkStatus,    // exit with success
    ExceptionalTerminated, // exit with failure  TODO: refine with `ForceTerminated` and `ExitedPanic`?
    WaitingInput,          // the command reqeust input when it is running
}

pub struct CommandRunner {
    child: Child,
    output_receiver: UnboundedReceiver<String>,
    error_receiver: UnboundedReceiver<String>,
    input_sender: UnboundedSender<String>,
    is_terminated: Arc<AtomicBool>,
    input_ready: Arc<Notify>,
}

impl CommandRunner {
    pub async fn run(command: &str) -> Result<Self> {
        let (output_sender, output_receiver) = unbounded_channel();
        let (error_sender, error_receiver) = unbounded_channel();
        let (input_sender, mut input_receiver) = unbounded_channel::<String>();

        let parts: Vec<&str> = command.split_whitespace().collect();
        let (cmd, args) = if parts.len() > 1 {
            (parts[0], &parts[1..])
        } else {
            (parts[0], &[][..])
        };

        let mut child = Command::new(cmd)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("Failed to spawn command")?;

        let mut stdin = child.stdin.take().context("Failed to capture stdin")?;
        let stdout = child.stdout.take().context("Failed to capture stdout")?;
        let stderr = child.stderr.take().context("Failed to capture stderr")?;

        let is_terminated = Arc::new(AtomicBool::new(false));

        // stdout
        let is_terminated_clone = Arc::clone(&is_terminated);
        tokio::spawn(async move {
            Self::read_stream(stdout, output_sender, is_terminated_clone).await;
        });
        // stderr
        let is_terminated_clone = Arc::clone(&is_terminated);
        tokio::spawn(async move {
            Self::read_stream(stderr, error_sender, is_terminated_clone).await;
        });
        // stdin
        let is_terminated_clone = Arc::clone(&is_terminated);
        let input_ready = Arc::new(Notify::new());
        let input_ready_clone = Arc::clone(&input_ready);
        tokio::spawn(async move {
            while !is_terminated_clone.load(Ordering::Relaxed) {
                tokio::select! {
                    Some(input) = input_receiver.recv() => {
                        if let Err(e) = stdin.write_all(input.as_bytes()).await {
                            eprintln!("Error writing to stdin: {}", e);
                            continue;
                        }
                        if let Err(e) = stdin.flush().await {
                            eprintln!("Error flushing stdin: {}", e);
                            continue;
                        }
                    }
                    _ = input_ready_clone.notified() => {
                        // Do nothing, just wake up to check termination flag
                    }
                }
            }
        });

        Ok(CommandRunner {
            child,
            output_receiver,
            error_receiver,
            input_sender,
            is_terminated,
            input_ready,
        })
    }

    async fn read_stream(
        stream: impl tokio::io::AsyncRead + Unpin,
        sender: UnboundedSender<String>,
        is_terminated: Arc<AtomicBool>,
    ) {
        let mut reader = BufReader::new(stream);
        let mut buffer = [0; 1024];
        let mut leftover = Vec::new();

        while !is_terminated.load(Ordering::Relaxed) {
            match reader.read(&mut buffer).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    leftover.extend_from_slice(&buffer[..n]);

                    // 查找并处理完整的行
                    while let Some(newline_pos) = leftover.iter().position(|&b| b == b'\n') {
                        let line = leftover.drain(..=newline_pos).collect::<Vec<_>>();
                        let (decoded, _, _) = GB18030.decode(&line);
                        let _ = sender.send(decoded.trim().into());
                    }
                }
                Err(_) => break,
            }
        }
    }

    pub async fn input(&self, input: &str) -> Result<()> {
        self.input_sender
            .send(input.to_string())
            .context("Failed to send input")?;
        self.input_ready.notify_one();
        Ok(())
    }

    pub async fn terminate(&mut self) {
        self.is_terminated.store(true, Ordering::Relaxed);
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }

    pub async fn get_status(&mut self) -> CommandStatus {
        match self.child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    CommandStatus::ExitedWithOkStatus
                } else {
                    CommandStatus::ExceptionalTerminated
                }
            }
            Ok(None) => {
                if self.is_ready_for_input().await {
                    CommandStatus::WaitingInput
                } else {
                    CommandStatus::Running
                }
            }
            Err(_) => CommandStatus::ExceptionalTerminated,
        }
    }

    async fn is_ready_for_input(&self) -> bool {
        // TODO: 这里需要一个更复杂的逻辑来检测是否准备好接受输入
        // 为了简化,我们假设总是准备好接受输入
        // true
        false

        // the sync version
        // if self.child.stdin.is_none() {
        //     return false;
        // }

        // let mut events = Events::with_capacity(1);
        // match self
        //     .poll
        //     .poll(&mut events, Some(std::time::Duration::from_millis(0)))
        // {
        //     Ok(_) => {
        //         for event in events.iter() {
        //             if event.token() == STDIN && event.is_writable() {
        //                 return true;
        //             }
        //         }
        //         false
        //     }
        //     Err(_) => false,
        // }
    }

    pub async fn get_one_output(&mut self) -> Option<String> {
        self.output_receiver.recv().await
    }

    pub async fn get_one_error(&mut self) -> Option<String> {
        self.error_receiver.recv().await
    }
}
