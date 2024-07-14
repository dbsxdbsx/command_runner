#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum CommandStatus {
    Running, // TODO: refine it further to a struct with method `is_waiting_for_input`
    // including finished witn normal exit code or error exit code or force terminated
    Stopped, // TODO: refine it further to a struct with method `is_exit_with_error` and `is_exit_with_ok`
}
