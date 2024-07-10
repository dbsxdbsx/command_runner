#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum CommandStatus {
    Running,
    ExitedWithOkStatus,
    ExceptionalTerminated,
    WaitingInput,
}
