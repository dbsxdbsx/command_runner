mod test;
use crossbeam::channel::{unbounded, Receiver, Sender};
use encoding_rs::GB18030;
use std::io::Read;

#[cfg(not(windows))]
use mio::unix::pipe::Receiver as UnixReceiver;
#[cfg(windows)]
use mio::windows::NamedPipe;
use std::os::windows::io::{IntoRawHandle, RawHandle};

#[cfg(windows)]
fn create_named_pipe<T: IntoRawHandle>(handle: T) -> std::io::Result<NamedPipe> {
    use std::os::windows::io::FromRawHandle;

    let raw_handle: RawHandle = handle.into_raw_handle();
    unsafe { Ok(NamedPipe::from_raw_handle(raw_handle)) }
}

use mio::{Events, Interest, Poll, Token};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{fmt, thread};

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum CommandStatus {
    Running,
    ExitedWithOkStatus,
    ExceptionalTerminated,
    WaitingInput,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OutputType {
    StdOut,
    StdErr,
}
impl fmt::Display for OutputType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            OutputType::StdOut => write!(f, "stdout"),
            OutputType::StdErr => write!(f, "stderr"),
        }
    }
}

#[derive(Debug)]
pub struct Output {
    output_type: OutputType,
    content: String,
}
impl fmt::Display for Output {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.output_type, self.content)
    }
}

impl Output {
    pub fn new(output_type: OutputType, content: String) -> Self {
        Self {
            output_type,
            content,
        }
    }

    pub fn as_str(&self) -> &str {
        self.content.as_str()
    }

    pub fn get_type(&self) -> OutputType {
        self.output_type
    }

    pub fn is_err(&self) -> bool {
        self.output_type == OutputType::StdErr
    }
}

pub struct CommandRunner {
    output_receiver: Receiver<Output>,
    child: Arc<Mutex<Child>>,
    status: Arc<Mutex<CommandStatus>>,
    is_terminated: Arc<AtomicBool>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl CommandRunner {
    pub fn run(command: &str) -> std::io::Result<Self> {
        let (output_sender, output_receiver) = unbounded();

        let parts: Vec<&str> = command.split_whitespace().collect();
        let (cmd, args) = if parts.len() > 1 {
            (parts[0], &parts[1..])
        } else {
            (parts[0], &[][..])
        };
        let child = Arc::new(Mutex::new(
            Command::new(cmd)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?,
        ));

        let stdout = child.lock().unwrap().stdout.take().unwrap();
        let stderr = child.lock().unwrap().stderr.take().unwrap();

        #[cfg(unix)]
        let stdout_receiver = UnixReceiver::from(stdout);
        #[cfg(unix)]
        let stderr_receiver = UnixReceiver::from(stderr);

        #[cfg(windows)]
        let mut stdout_receiver = create_named_pipe(stdout)?;
        #[cfg(windows)]
        let mut stderr_receiver = create_named_pipe(stderr)?;

        let status = Arc::new(Mutex::new(CommandStatus::Running));
        let is_terminated = Arc::new(AtomicBool::new(false));

        let status_clone = Arc::clone(&status);
        let child_clone = Arc::clone(&child);
        let is_terminated_clone = Arc::clone(&is_terminated);

        let handle = thread::spawn(move || {
            let mut poll = Poll::new().unwrap();
            let mut events = Events::with_capacity(1024);

            let stdout_token = Token(1);
            let stderr_token = Token(2);

            poll.registry()
                .register(&mut stdout_receiver, stdout_token, Interest::READABLE)
                .unwrap();
            poll.registry()
                .register(&mut stderr_receiver, stderr_token, Interest::READABLE)
                .unwrap();

            let mut stdout_buffer = [0; 1024];
            let mut stderr_buffer = [0; 1024];

            loop {
                if is_terminated_clone.load(Ordering::Relaxed) {
                    break;
                }

                poll.poll(&mut events, Some(Duration::from_millis(10)))
                    .unwrap();

                for event in events.iter() {
                    match event.token() {
                        Token(1) => {
                            if let Ok(n) = stdout_receiver.read(&mut stdout_buffer) {
                                if n > 0 {
                                    process_stream(&output_sender, &mut stdout_buffer[..n], false);
                                }
                            }
                        }
                        Token(2) => {
                            if let Ok(n) = stderr_receiver.read(&mut stderr_buffer) {
                                if n > 0 {
                                    process_stream(&output_sender, &mut stderr_buffer[..n], true);
                                }
                            }
                        }
                        _ => unreachable!(),
                    }
                }

                match child_clone.lock().unwrap().try_wait() {
                    Ok(Some(status)) => {
                        let mut command_status = status_clone.lock().unwrap();
                        *command_status = if status.success() {
                            CommandStatus::ExitedWithOkStatus
                        } else {
                            CommandStatus::ExceptionalTerminated
                        };
                        is_terminated_clone.store(true, Ordering::Relaxed);
                        break;
                    }
                    Ok(None) => continue,
                    Err(_) => {
                        let mut command_status = status_clone.lock().unwrap();
                        *command_status = CommandStatus::ExceptionalTerminated;
                        is_terminated_clone.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
        });

        Ok(CommandRunner {
            output_receiver,
            child,
            status,
            is_terminated,
            thread_handle: Some(handle),
        })
    }

    pub fn terminate(&mut self) -> std::io::Result<()> {
        // set status
        self.is_terminated.store(true, Ordering::Relaxed);
        let mut command_status = self.status.lock().unwrap();
        *command_status = CommandStatus::ExceptionalTerminated;

        // wait for thread to exit
        if let Some(handle) = self.thread_handle.take() {
            handle.join().unwrap();
        }

        // send terminate signal to child process
        self.child.lock().unwrap().kill()?;
        let _ = self.child.lock().unwrap().wait()?;

        Ok(())
    }

    pub fn get_status(&self) -> CommandStatus {
        *self.status.lock().unwrap()
    }

    pub fn get_one_line_output(&self) -> Option<Output> {
        self.output_receiver.try_iter().next()
    }
}

fn process_stream(sender: &Sender<Output>, buffer: &[u8], is_stderr: bool) {
    let mut leftover = Vec::new();
    leftover.extend_from_slice(buffer);

    // find and process complete lines
    while let Some(newline_pos) = leftover.iter().position(|&b| b == b'\n') {
        let line = leftover.drain(..=newline_pos).collect::<Vec<_>>();
        let (decoded, _, _) = GB18030.decode(&line);
        let output = if is_stderr {
            Output::new(OutputType::StdErr, decoded.trim().to_owned())
        } else {
            Output::new(OutputType::StdOut, decoded.trim().to_owned())
        };
        sender.send(output).unwrap();
    }
}
