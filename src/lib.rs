mod test;

use crossbeam::channel::{unbounded, Receiver, Sender};
use encoding_rs::GB18030;
use mio::{Events, Interest, Poll, Token};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::thread::{self, JoinHandle};

#[cfg(any(unix, target_os = "android"))]
use mio::unix::SourceFd;
#[cfg(any(unix, target_os = "android"))]
use std::os::unix::io::AsRawFd;

#[cfg(windows)]
use mio::windows::NamedPipe;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle};

const STDIN: Token = Token(0);
const TERMINATE_COMMAND: &str = "__TERMINATE_COMMAND_ONLY_FOR_CRATE_COMMAND_RUNNER__";

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CommandStatus {
    Running,
    Finished,
    WaitingForInput,
    ExceptionTerminated,
}

pub struct CommandRunner {
    child: Child,
    output_receiver: Receiver<String>,
    error_receiver: Receiver<String>,
    input_sender: Sender<String>,
    thread_handles: Vec<JoinHandle<()>>,
    poll: Poll,
}

impl CommandRunner {
    pub fn run(command: &str) -> Result<Self, std::io::Error> {
        let (output_sender, output_receiver) = unbounded();
        let (error_sender, error_receiver) = unbounded();
        let (input_sender, input_receiver) = unbounded();

        let parts: Vec<&str> = command.split_whitespace().collect();
        let (cmd, args) = if parts.len() > 1 {
            (parts[0], &parts[1..])
        } else {
            (parts[0], &[][..])
        };

        let mut child = Command::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take().expect("Failed to capture stdin");
        let stdout = child.stdout.take().expect("Failed to capture stdout");
        let stderr = child.stderr.take().expect("Failed to capture stderr");

        let stdout_handle = thread::spawn(move || Self::read_stream(stdout, output_sender));
        let stderr_handle = thread::spawn(move || Self::read_stream(stderr, error_sender));
        let stdin_handle = thread::spawn(move || Self::write_stream(stdin, input_receiver));

        // poll of mio
        let poll = Poll::new()?;
        #[cfg(windows)]
        {
            if let Some(stdin) = child.stdin.as_ref() {
                let stdin_handle = stdin.as_raw_handle();
                let mut pipe = unsafe { NamedPipe::from_raw_handle(stdin_handle) };
                poll.registry()
                    .register(&mut pipe, STDIN, Interest::WRITABLE)?;
            }
        }
        #[cfg(any(unix, target_os = "android"))]
        {
            if let Some(stdin) = child.stdin.as_ref() {
                let stdin_fd = stdin.as_raw_fd();
                poll.registry()
                    .register(&mut SourceFd(&stdin_fd), STDIN, Interest::WRITABLE)?;
            }
        }

        Ok(CommandRunner {
            child,
            output_receiver,
            error_receiver,
            input_sender,
            thread_handles: vec![stdout_handle, stderr_handle, stdin_handle],
            poll,
        })
    }

    fn read_stream<R: std::io::Read>(stream: R, sender: crossbeam::channel::Sender<String>) {
        let mut reader = BufReader::new(stream);
        let mut buffer = [0; 1024];
        let mut leftover = Vec::new();

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => {
                    // continue;
                    // 处理剩余数据
                    //  TODO: refine messy code issue
                    if !leftover.is_empty() {
                        let (decoded, _, _) = GB18030.decode(&leftover);
                        let _ = sender.send(decoded.into_owned());
                    }
                    // break;
                }
                Ok(n) => {
                    leftover.extend_from_slice(&buffer[..n]);

                    // find and process complete lines
                    while let Some(newline_pos) = leftover.iter().position(|&b| b == b'\n') {
                        let line = leftover.drain(..=newline_pos).collect::<Vec<_>>();
                        let (decoded, _, _) = GB18030.decode(&line);
                        let _ = sender.send(decoded.into_owned());
                    }

                    // clear after usage
                    buffer.fill(0);
                }
                Err(_) => break,
            }
        }
    }

    fn write_stream<W: std::io::Write>(mut stream: W, receiver: Receiver<String>) {
        for input in receiver.iter() {
            if input == TERMINATE_COMMAND {
                break;
            }
            if let Err(e) = writeln!(stream, "{}", input) {
                eprintln!("Error writing to stdin: {}", e);
                break;
            }
        }
    }

    pub fn input(&self, input: &str) -> Result<(), std::io::Error> {
        self.input_sender.send(input.to_string()).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to send input: {}", e),
            )
        })
    }

    pub fn terminate(&mut self) {
        let _ = self.input_sender.send(TERMINATE_COMMAND.to_string());
        let _ = self.child.kill();
        let _ = self.child.wait();

        for handle in self.thread_handles.drain(..) {
            if let Err(e) = handle.join() {
                eprintln!("Error joining thread: {:?}", e);
            }
        }
    }

    pub fn get_status(&mut self) -> CommandStatus {
        match self.child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    CommandStatus::Finished
                } else {
                    CommandStatus::ExceptionTerminated
                }
            }
            Ok(None) => {
                if self.is_ready_for_input() {
                    CommandStatus::WaitingForInput
                } else {
                    CommandStatus::Running
                }
            }
            Err(_) => CommandStatus::ExceptionTerminated,
        }
    }

    fn is_ready_for_input(&mut self) -> bool {
        if self.child.stdin.is_none() {
            return false;
        }

        let mut events = Events::with_capacity(1);
        match self
            .poll
            .poll(&mut events, Some(std::time::Duration::from_millis(0)))
        {
            Ok(_) => {
                for event in events.iter() {
                    if event.token() == STDIN && event.is_writable() {
                        return true;
                    }
                }
                false
            }
            Err(_) => false,
        }
    }

    pub fn get_output(&self) -> Vec<String> {
        self.output_receiver.try_iter().collect()
    }

    pub fn get_error(&self) -> Vec<String> {
        self.error_receiver.try_iter().collect()
    }
}

impl Drop for CommandRunner {
    fn drop(&mut self) {
        self.terminate();
    }
}
