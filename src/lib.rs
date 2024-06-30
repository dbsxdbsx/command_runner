use crossbeam::channel::{unbounded, Receiver};
use encoding_rs::GB18030;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::thread::{self, JoinHandle};
mod test;

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
    thread_handles: Vec<JoinHandle<()>>,
}

impl CommandRunner {
    pub fn run(command: &str, args: &[&str]) -> Result<Self, std::io::Error> {
        let (output_sender, output_receiver) = unbounded();
        let (error_sender, error_receiver) = unbounded();

        let mut child = Command::new(command)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().expect("Failed to capture stdout");
        let stderr = child.stderr.take().expect("Failed to capture stderr");

        let stdout_handle = thread::spawn(move || Self::read_stream(stdout, output_sender));
        let stderr_handle = thread::spawn(move || Self::read_stream(stderr, error_sender));

        Ok(CommandRunner {
            child,
            output_receiver,
            error_receiver,
            thread_handles: vec![stdout_handle, stderr_handle],
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

    pub fn terminate(&mut self) {
        // 尝试终止子进程
        let _ = self.child.kill();
        let _ = self.child.wait();

        // 等待所有流处理线程完成
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
