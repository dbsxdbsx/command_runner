mod test;
use crossbeam::channel::{unbounded, Receiver, Sender};
use encoding_rs::GB18030;
use std::io::{BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct CommandRunner {
    stdout_receiver: Receiver<String>,
    stderr_receiver: Receiver<String>,
    child: Arc<Mutex<Child>>,
    status: Arc<Mutex<CommandStatus>>,
}

#[derive(Clone, Copy, Debug)]
pub enum CommandStatus {
    Running,
    ExitedWithOkStatus,
    ExceptionalTerminated,
    WaitingInput,
}

impl CommandRunner {
    pub fn run(command: &str) -> std::io::Result<Self> {
        let (stdout_sender, stdout_receiver) = unbounded();
        let (stderr_sender, stderr_receiver) = unbounded();

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

        let status = Arc::new(Mutex::new(CommandStatus::Running));
        let is_terminated = Arc::new(AtomicBool::new(false));

        let status_clone = Arc::clone(&status);
        let child_clone = Arc::clone(&child);
        let is_terminated_clone = Arc::clone(&is_terminated);

        thread::spawn(move || {
            let mut stdout_reader = BufReader::new(stdout);
            let mut stderr_reader = BufReader::new(stderr);
            let mut stdout_buffer = [0; 1024];
            let mut stderr_buffer = [0; 1024];

            loop {
                let mut stdout_read = 0;
                let mut stderr_read = 0;

                if let Ok(n) = stdout_reader.read(&mut stdout_buffer) {
                    stdout_read = n;
                    if n > 0 {
                        process_stream(&stdout_sender, &mut stdout_buffer[..n]);
                    }
                }

                if let Ok(n) = stderr_reader.read(&mut stderr_buffer) {
                    stderr_read = n;
                    if n > 0 {
                        process_stream(&stderr_sender, &mut stderr_buffer[..n]);
                    }
                }

                if stdout_read == 0 && stderr_read == 0 {
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
            }
        });

        Ok(CommandRunner {
            stdout_receiver,
            stderr_receiver,
            child,
            status,
        })
    }

    pub fn terminate(&self) -> std::io::Result<()> {
        self.child.lock().unwrap().kill()
    }

    pub fn get_status(&self) -> CommandStatus {
        *self.status.lock().unwrap()
    }

    pub fn get_one_line_output(&self) -> Option<String> {
        self.stdout_receiver.try_iter().next()
    }

    pub fn get_one_line_error(&self) -> Option<String> {
        self.stderr_receiver.try_iter().next()
    }
}

fn process_stream(sender: &Sender<String>, buffer: &[u8]) {
    let mut leftover = Vec::new();
    leftover.extend_from_slice(buffer);

    // find and process complete lines
    while let Some(newline_pos) = leftover.iter().position(|&b| b == b'\n') {
        let line = leftover.drain(..=newline_pos).collect::<Vec<_>>();
        let (decoded, _, _) = GB18030.decode(&line);
        sender.send(decoded.into_owned()).unwrap();
    }
}
