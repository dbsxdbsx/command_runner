use crossbeam::channel::{unbounded, Receiver, Sender, TryRecvError};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct CLI {
    stdout_receiver: Receiver<String>,
    stderr_receiver: Receiver<String>,
    child: Arc<Mutex<Child>>,
    status: Arc<Mutex<CommandStatus>>,
    output: Arc<Mutex<String>>,
    error: Arc<Mutex<String>>,
}

#[derive(Clone, Copy, Debug)]
pub enum CommandStatus {
    Running,
    ExitedWithOkStatus,
    ExceptionalTerminated,
}

impl CLI {
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
        let output = Arc::new(Mutex::new(String::new()));
        let error = Arc::new(Mutex::new(String::new()));

        let status_clone = Arc::clone(&status);
        let output_clone = Arc::clone(&output);
        let error_clone = Arc::clone(&error);
        let child_clone = Arc::clone(&child);

        thread::spawn(move || {
            let mut stdout_reader = std::io::BufReader::new(stdout);
            let mut stderr_reader = std::io::BufReader::new(stderr);

            loop {
                use std::io::BufRead;

                let mut stdout_line = String::new();
                let mut stderr_line = String::new();

                match stdout_reader.read_line(&mut stdout_line) {
                    Ok(0) => break,
                    Ok(_) => {
                        stdout_sender.send(stdout_line.clone()).unwrap();
                        output_clone.lock().unwrap().push_str(&stdout_line);
                    }
                    Err(_) => break,
                }

                match stderr_reader.read_line(&mut stderr_line) {
                    Ok(0) => break,
                    Ok(_) => {
                        stderr_sender.send(stderr_line.clone()).unwrap();
                        error_clone.lock().unwrap().push_str(&stderr_line);
                    }
                    Err(_) => break,
                }

                match child_clone.lock().unwrap().try_wait() {
                    Ok(Some(status)) => {
                        let mut command_status = status_clone.lock().unwrap();
                        *command_status = if status.success() {
                            CommandStatus::ExitedWithOkStatus
                        } else {
                            CommandStatus::ExceptionalTerminated
                        };
                        break;
                    }
                    Ok(None) => continue,
                    Err(_) => {
                        let mut command_status = status_clone.lock().unwrap();
                        *command_status = CommandStatus::ExceptionalTerminated;
                        break;
                    }
                }
            }
        });

        Ok(CLI {
            stdout_receiver,
            stderr_receiver,
            child,
            status,
            output,
            error,
        })
    }

    pub fn terminate(&self) -> std::io::Result<()> {
        self.child.lock().unwrap().kill()
    }

    pub fn get_status(&self) -> CommandStatus {
        *self.status.lock().unwrap()
    }

    pub fn get_one_output(&self) -> Option<String> {
        self.stdout_receiver.try_iter().next()
    }

    pub fn get_one_error(&self) -> Option<String> {
        self.stderr_receiver.try_iter().next()
    }
}
