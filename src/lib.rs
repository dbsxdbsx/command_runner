mod test;

use crossbeam::channel::{unbounded, Receiver, RecvTimeoutError, Sender, TryRecvError};
use encoding_rs::GB18030;
use mio::{Events, Interest, Poll, Token};
use std::io::{BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[cfg(any(unix, target_os = "android"))]
use mio::unix::SourceFd;
#[cfg(any(unix, target_os = "android"))]
use std::os::unix::io::AsRawFd;

#[cfg(windows)]
use mio::windows::NamedPipe;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle};

const STDIN: Token = Token(0);

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CommandStatus {
    Running,               // the command is valid initialized and is running
    ExitedWithOkStatus,    // exit with success
    ExceptionalTerminated, // exit with failure  TODO: refine with `ForceTerminated` and `ExitedPanic`?
    WaitingInput,          // the command reqeust input when it is running
}

pub struct CommandRunner {
    child: Child,
    output_receiver: Receiver<String>,
    error_receiver: Receiver<String>,
    input_sender: Sender<String>,
    thread_handles: Vec<JoinHandle<()>>,
    poll: Poll,
    is_terminated: Arc<AtomicBool>,
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

        // atomic status flag required to be stored in instance field
        let is_terminated = Arc::new(AtomicBool::new(false));
        let is_terminated_clone = Arc::clone(&is_terminated);

        // for std out
        let stdout_handle =
            thread::spawn(move || Self::read_stream(stdout, output_sender, is_terminated_clone));

        // for std error
        let is_terminated_clone = Arc::clone(&is_terminated);
        let stderr_handle =
            thread::spawn(move || Self::read_stream(stderr, error_sender, is_terminated_clone));

        // for std input
        let is_terminated_clone = Arc::clone(&is_terminated);
        let stdin_handle =
            thread::spawn(move || Self::write_stream(stdin, input_receiver, is_terminated_clone));

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
            is_terminated,
        })
    }

    fn read_stream<R: std::io::Read>(
        stream: R,
        sender: crossbeam::channel::Sender<String>,
        is_terminated: Arc<AtomicBool>,
    ) {
        let mut reader = BufReader::new(stream);
        let mut buffer = [0; 1024];
        let mut leftover = Vec::new();

        while !is_terminated.load(Ordering::Relaxed) {
            match reader.read(&mut buffer) {
                Ok(n) => {
                    leftover.extend_from_slice(&buffer[..n]);

                    // find and process complete lines
                    while let Some(newline_pos) = leftover.iter().position(|&b| b == b'\n') {
                        let line = leftover.drain(..=newline_pos).collect::<Vec<_>>();
                        let (decoded, _, _) = GB18030.decode(&line);
                        sender.send(decoded.into_owned()).unwrap();
                    }
                }
                Err(_) => break,
            }
        }
    }

    fn write_stream<W: std::io::Write>(
        mut stream: W,
        receiver: Receiver<String>,
        is_terminated: Arc<AtomicBool>,
    ) {
        while !is_terminated.load(Ordering::Relaxed) {
            match receiver.try_recv() {
                Ok(input) => {
                    if let Err(e) = writeln!(stream, "{}", input) {
                        eprintln!("Error writing to stdin: {}", e);
                        continue; // 继续尝试,而不是退出
                    }
                    if let Err(e) = stream.flush() {
                        eprintln!("Error flushing stdin: {}", e);
                        continue;
                    }
                }
                Err(TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(10)); // 短暂休眠,减少CPU使用
                    continue;
                }
                Err(TryRecvError::Disconnected) => break,
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
        // 设置终止标志
        self.is_terminated.store(true, Ordering::Relaxed);

        let _ = self.child.kill();
        let _ = self.child.wait();

        for handle in self.thread_handles.drain(..) {
            handle.join().unwrap()
        }
    }

    pub fn get_status(&mut self) -> CommandStatus {
        match self.child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    CommandStatus::ExitedWithOkStatus
                } else {
                    CommandStatus::ExceptionalTerminated
                }
            }
            Ok(None) => {
                if self.is_ready_for_input() {
                    CommandStatus::WaitingInput
                } else {
                    CommandStatus::Running
                }
            }
            Err(_) => CommandStatus::ExceptionalTerminated,
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
