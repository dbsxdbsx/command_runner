mod output;
mod status;
mod test;
use crate::output::{Output, OutputType};
use crate::status::CommandStatus;

use anyhow::Result;
use crossbeam::channel::{unbounded, Receiver, Sender};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

pub struct CommandRunner {
    child: Arc<Mutex<Child>>,
    output_receiver: Receiver<Output>,
    status: Arc<Mutex<CommandStatus>>,
    input_sender: Sender<String>,
    thread: Option<JoinHandle<()>>,
}

impl CommandRunner {
    pub fn run(command: &str) -> Result<Self> {
        let (output_sender, output_receiver) = unbounded();
        let (input_sender, input_receiver) = unbounded();
        let status = Arc::new(Mutex::new(CommandStatus::Running));

        let parts: Vec<&str> = command.split_whitespace().collect();
        let (cmd, args) = if parts.len() > 1 {
            (parts[0], &parts[1..])
        } else {
            (parts[0], &[][..])
        };
        let child_arc = Arc::new(Mutex::new(
            Command::new(cmd)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?,
        ));

        let sender = output_sender.clone();
        let child = Arc::clone(&child_arc);
        let status_clone = Arc::clone(&status);
        let thread = thread::spawn({
            // 处理stdout和stderr
            let stdout = child.lock().unwrap().stdout.take().unwrap();
            let stderr = child.lock().unwrap().stderr.take().unwrap();
            let stdout_reader = BufReader::new(stdout);
            let stderr_reader = BufReader::new(stderr);

            let mut stdout_lines = stdout_reader.lines();
            let mut stderr_lines = stderr_reader.lines();

            let tmp_status = *status_clone.lock().unwrap();
            let is_terminated = tmp_status == CommandStatus::ExceptionalTerminated
                || tmp_status == CommandStatus::ExitedWithOkStatus;
            while !is_terminated {
                match (stdout_lines.next(), stderr_lines.next()) {
                    (Some(Ok(line)), _) => {
                        sender.send(Output::new(OutputType::StdOut, line)).unwrap();
                    }
                    (_, Some(Ok(line))) => {
                        sender.send(Output::new(OutputType::StdErr, line)).unwrap();
                    }
                    (None, None) => break,
                    _ => {}
                }
            }

            // TODO:处理stdin
            // let stdin_handle = thread::spawn({
            //     let child = Arc::clone(&child_arc);
            //     let status = Arc::clone(&status_clone);
            //     move || {
            //         let mut stdin = child.lock().unwrap().stdin.take().unwrap();
            //         for input in input_receiver {
            //             if let Err(_) = writeln!(stdin, "{}", input) {
            //                 break;
            //             }
            //             *status.lock().unwrap() = CommandStatus::Running;
            //         }
            //     }
            // });

            // 监控子进程状态
            let child = Arc::clone(&child_arc);
            let status = Arc::clone(&status_clone);
            move || {
                let exit_status = child.lock().unwrap().wait().unwrap();
                if exit_status.success() {
                    *status.lock().unwrap() = CommandStatus::ExitedWithOkStatus;
                } else {
                    *status.lock().unwrap() = CommandStatus::ExceptionalTerminated;
                }
            }
        });

        Ok(Self {
            child: child_arc,
            output_receiver,
            status: status_clone,
            input_sender,
            thread: Some(thread),
        })
    }

    pub fn terminate(&mut self) {
        // update status
        *self.status.lock().unwrap() = CommandStatus::ExceptionalTerminated;

        // wait for thread to exit
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
        // send terminate signal to child process
        self.child.lock().unwrap().kill().unwrap();
        let _ = self.child.lock().unwrap().wait().unwrap();
    }

    pub fn get_status(&self) -> CommandStatus {
        *self.status.lock().unwrap()
    }

    pub fn get_one_line_output(&self) -> Option<Output> {
        self.output_receiver.try_recv().ok()
    }

    // TODO: 实现input方法
    // pub fn input(&self, input: &str) -> Result<()> {
    //     self.input_sender.send(input.to_string())?;
    //     Ok(())
    // }
}

impl Drop for CommandRunner {
    fn drop(&mut self) {
        self.terminate();
    }
}
