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
use std::thread;
use std::time::Duration;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum CommandStatus {
    Running,
    ExitedWithOkStatus,
    ExceptionalTerminated,
    WaitingInput,
}

#[derive(Debug, PartialEq, Eq)]
pub enum OutputType {
    StdOut,
    StdErr,
}

impl Output {
    pub fn get_type(&self) -> OutputType {
        match self {
            Output::StdOut(_) => OutputType::StdOut,
            Output::StdErr(_) => OutputType::StdErr,
        }
    }
}

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum Output {
    StdOut(String),
    StdErr(String),
}

impl Output {
    pub fn as_str(&self) -> &str {
        match self {
            Output::StdOut(s) => s.as_str(),
            Output::StdErr(s) => s.as_str(),
        }
    }
}

impl PartialEq<&str> for Output {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<String> for Output {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl fmt::Display for Output {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Output::StdOut(s) => write!(f, "StdOut: {}", s),
            Output::StdErr(s) => write!(f, "StdErr: {}", s),
        }
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
            Output::StdErr(decoded.trim().to_owned())
        } else {
            Output::StdOut(decoded.trim().to_owned())
        };
        sender.send(output).unwrap();
    }
}
