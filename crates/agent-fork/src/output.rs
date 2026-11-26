use std::fmt;

pub trait ForkOutput: Send + 'static {
    fn line(&mut self, line: &str);

    fn error_line(&mut self, _line: &str) {}

    fn ended(self: Box<Self>, exit: Exit);
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Exit {
    pub status: String,
    pub stderr: String,
}

impl fmt::Display for Exit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stderr = self.stderr.trim();
        if stderr.is_empty() {
            write!(formatter, "{}", self.status)
        } else {
            write!(formatter, "{} ({stderr})", self.status)
        }
    }
}
