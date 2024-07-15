mod output;
mod status;
mod test;
use crate::output::{Output, OutputType};
use crate::status::CommandStatus;

use anyhow::Result;
use crossbeam::channel::{unbounded, Receiver, Sender};
use encoding_rs::GB18030;
use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

pub struct CommandRunner {
    command: Command,
    child: Arc<Mutex<Option<Child>>>,
    output_sender: Sender<Output>,
    output_receiver: Receiver<Output>,
    threads: Vec<JoinHandle<()>>,
    force_stop: Arc<AtomicBool>,
}

impl CommandRunner {
    pub fn new(command: &str) -> Result<Self> {
        // init command
        let parts: Vec<&str> = command.split_whitespace().collect();
        let (cmd_root, cmd_args) = if parts.len() > 1 {
            (parts[0], &parts[1..])
        } else {
            (parts[0], &[][..])
        };
        let mut command = Command::new(cmd_root);
        command
            .args(cmd_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // init output channel
        let (output_sender, output_receiver) = unbounded();

        // use `.spawn()` to check it the command is valid, if valid, termiate it immediately.
        let mut child = command.spawn()?;
        child.kill().unwrap();
        child.wait().unwrap();

        // return new instance
        Ok(Self {
            command,
            child: Arc::new(Mutex::new(None)),
            output_sender,
            output_receiver,
            threads: Vec::new(),
            force_stop: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn run(&mut self) {
        // 停止之前的进程(如果存在)
        self.stop();

        // 初始化子进程和相关字段
        let mut child = self.command.spawn().unwrap();
        self.force_stop
            .store(false, std::sync::atomic::Ordering::SeqCst);

        // 对于 stdout 流
        let force_stop_for_stdout = Arc::clone(&self.force_stop);
        let stdout = child.stdout.take().unwrap();
        let stdout_child = Arc::clone(&self.child);
        let stdout_sender = self.output_sender.clone();
        let stdout_thread = thread::spawn(move || {
            process_stream(
                stdout,
                &stdout_sender,
                false,
                stdout_child,
                force_stop_for_stdout,
            );
        });

        // 对于 stderr 流
        let force_stop_for_stderr = Arc::clone(&self.force_stop);
        let stderr = child.stderr.take().unwrap();
        let stderr_child = Arc::clone(&self.child);
        let stderr_sender = self.output_sender.clone();
        let stderr_thread = thread::spawn(move || {
            process_stream(
                stderr,
                &stderr_sender,
                true,
                stderr_child,
                force_stop_for_stderr,
            );
        });

        // 收集线程
        self.threads.push(stdout_thread);
        self.threads.push(stderr_thread);

        // update child process
        *self.child.lock().unwrap() = Some(child);
    }

    pub fn stop(&mut self) {
        // set force stop flag
        self.force_stop
            .store(true, std::sync::atomic::Ordering::SeqCst);

        // wait for threads to finish
        for thread in self.threads.drain(..) {
            thread.join().unwrap();
        }

        // kill child process first
        if let Some(child) = self.child.lock().unwrap().as_mut() {
            child.kill().unwrap();
            child.wait().unwrap();
        }

        // clear threads vec
        self.threads.clear();
    }

    pub fn is_stopped(&self) -> bool {
        matches!(self.check_status().unwrap(), CommandStatus::Stopped)
    }

    pub fn is_running(&self) -> bool {
        matches!(self.check_status().unwrap(), CommandStatus::Running)
    }

    fn check_status(&self) -> Result<CommandStatus, String> {
        if self.child.lock().unwrap().is_none() {
            return Err("Child process is not initialized yet.".to_string());
        }
        Ok(check_child_process_status(&self.child))
    }

    pub fn get_one_line_output(&self) -> Option<Output> {
        self.output_receiver.try_recv().ok()
    }
}

impl Drop for CommandRunner {
    fn drop(&mut self) {
        self.stop();
    }
}

fn process_stream<R: Read>(
    mut stream: R,
    sender: &Sender<Output>,
    is_stderr: bool,
    child: Arc<Mutex<Option<Child>>>,
    force_stop: Arc<AtomicBool>,
) {
    let mut buffer = [0; 1024];
    let mut leftover = Vec::new();

    while !force_stop.load(std::sync::atomic::Ordering::SeqCst)
        && check_child_process_status(&child) != CommandStatus::Stopped
    {
        match stream.read(&mut buffer) {
            Ok(0) => break, // 流结束
            Ok(n) => {
                leftover.extend_from_slice(&buffer[..n]);
                process_buffer(sender, &mut leftover, is_stderr);
            }
            Err(_) => break, // 读取错误
        }
    }
}

fn process_buffer(sender: &Sender<Output>, buffer: &mut Vec<u8>, is_stderr: bool) {
    while let Some(newline_pos) = buffer.iter().position(|&b| b == b'\n') {
        let line = buffer.drain(..=newline_pos).collect::<Vec<_>>();
        let (decoded, _, _) = GB18030.decode(&line);
        let output = if is_stderr {
            Output::new(OutputType::StdErr, decoded.trim().to_owned())
        } else {
            Output::new(OutputType::StdOut, decoded.trim().to_owned())
        };
        sender.send(output).unwrap();
    }
}

fn check_child_process_status(child: &Arc<Mutex<Option<Child>>>) -> CommandStatus {
    let mut status = CommandStatus::Stopped;
    if let Ok(mut child_guard) = child.lock() {
        if let Some(child) = child_guard.as_mut() {
            if let Ok(result) = child.try_wait() {
                match result {
                    Some(_) => status = CommandStatus::Stopped,
                    None => status = CommandStatus::Running,
                }
            }
        }
    }
    status
}
