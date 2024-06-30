use crossbeam::channel::{unbounded, Receiver, Sender};
use encoding_rs::GB18030;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::thread::{self, JoinHandle};
mod test;

const TERMINATE_COMMAND: &str = "__TERMINATE_COMMAND_ONLY_FOR_CRATE_COMMAND_RUNNER__";

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CommandStatus {
    Running,
    Finished,
    ExceptionTerminated,
}

pub struct CommandRunner {
    child: Child,
    output_receiver: Receiver<String>,
    error_receiver: Receiver<String>,
    input_sender: Sender<String>,
    thread_handles: Vec<JoinHandle<()>>,
}

impl CommandRunner {
    pub fn run(command: &str) -> Result<Self, std::io::Error> {
        let (output_sender, output_receiver) = unbounded();
        let (error_sender, error_receiver) = unbounded();
        let (input_sender, input_receiver) = unbounded();

        // Split commands and arguments
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

        Ok(CommandRunner {
            child,
            output_receiver,
            error_receiver,
            input_sender,
            thread_handles: vec![stdout_handle, stderr_handle, stdin_handle],
        })
    }

    fn read_stream<R: std::io::Read>(stream: R, sender: crossbeam::channel::Sender<String>) {
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            if let Ok(line) = line {
                let (decoded, _, _) = GB18030.decode(line.as_bytes());
                let _ = sender.send(decoded.into_owned());
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
        // Send a special termination signal to ensure the input thread can exit correctly
        let _ = self.input_sender.send(TERMINATE_COMMAND.to_string());

        // Attempt to terminate the child process
        let _ = self.child.kill();
        let _ = self.child.wait();

        // Wait for all stream handling threads to complete
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
            Ok(None) => CommandStatus::Running,
            Err(_) => CommandStatus::ExceptionTerminated,
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
