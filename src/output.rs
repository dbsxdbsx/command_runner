use std::fmt;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OutputType {
    StdOut,
    StdErr,
}
impl fmt::Display for OutputType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            OutputType::StdOut => write!(f, "stdout"),
            OutputType::StdErr => write!(f, "stderr"),
        }
    }
}

#[derive(Debug)]
pub struct Output {
    output_type: OutputType,
    content: String,
}
impl fmt::Display for Output {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.output_type, self.content)
    }
}

impl Output {
    pub fn new(output_type: OutputType, content: String) -> Self {
        Self {
            output_type,
            content,
        }
    }

    pub fn as_str(&self) -> &str {
        self.content.as_str()
    }

    pub fn get_type(&self) -> OutputType {
        self.output_type
    }

    pub fn is_err(&self) -> bool {
        self.output_type == OutputType::StdErr
    }
}
