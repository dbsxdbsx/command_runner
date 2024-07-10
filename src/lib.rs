mod output;
mod status;
mod test;
use crossbeam::channel::{unbounded, Receiver, Sender};
use encoding_rs::GB18030;
use output::{Output, OutputType};
use status::CommandStatus;
use std::io::{BufRead, BufReader};

#[cfg(not(windows))]
use mio::unix::pipe::Receiver as UnixReceiver;
#[cfg(windows)]
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct CommandRunner {
    output_receiver: Receiver<Output>,
    child: Arc<Mutex<Child>>,
    status: Arc<Mutex<CommandStatus>>,
    is_terminated: Arc<Mutex<bool>>,
    thread_handles: Vec<thread::JoinHandle<()>>,
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

        let status = Arc::new(Mutex::new(CommandStatus::Running));
        let is_terminated = Arc::new(Mutex::new(false));

        let mut thread_handles = Vec::new();

        // 创建 stdout 线程
        let stdout_sender = output_sender.clone();
        let stdout_is_terminated = Arc::clone(&is_terminated);
        let stdout_handle = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if *stdout_is_terminated.lock().unwrap() {
                    break;
                }
                if let Ok(line) = line {
                    let (decoded, _, _) = GB18030.decode(line.as_bytes());
                    let output = Output::new(OutputType::StdOut, decoded.into_owned());
                    stdout_sender.send(output).unwrap();
                }
            }
        });
        thread_handles.push(stdout_handle);

        // 创建 stderr 线程
        let stderr_sender = output_sender;
        let stderr_is_terminated = Arc::clone(&is_terminated);
        let stderr_handle = thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if *stderr_is_terminated.lock().unwrap() {
                    break;
                }
                if let Ok(line) = line {
                    let (decoded, _, _) = GB18030.decode(line.as_bytes());
                    let output = Output::new(OutputType::StdErr, decoded.into_owned());
                    stderr_sender.send(output).unwrap();
                }
            }
        });
        thread_handles.push(stderr_handle);

        Ok(CommandRunner {
            output_receiver,
            child,
            status,
            is_terminated,
            thread_handles,
        })
    }

    pub fn terminate(&mut self) -> std::io::Result<()> {
        // update status
        *self.is_terminated.lock().unwrap() = true;
        *self.status.lock().unwrap() = CommandStatus::ExceptionalTerminated;

        // wait for thread to exit
        for handle in self.thread_handles.drain(..) {
            handle.join().unwrap();
        }

        // send terminate signal to child process
        self.child.lock().unwrap().kill()?;
        let _ = self.child.lock().unwrap().wait()?;

        Ok(())
    }

    pub fn get_status(&self) -> CommandStatus {
        let mut child = self.child.lock().unwrap();
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    CommandStatus::ExitedWithOkStatus
                } else {
                    CommandStatus::ExceptionalTerminated
                }
            }
            Ok(None) => CommandStatus::Running,
            Err(_) => CommandStatus::ExceptionalTerminated,
        }
    }

    pub fn get_one_line_output(&self) -> Option<Output> {
        self.output_receiver.try_recv().ok()
    }
}

impl Drop for CommandRunner {
    fn drop(&mut self) {
        self.terminate().unwrap();
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
