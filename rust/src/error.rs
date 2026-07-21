use std::sync::mpsc;

#[derive(Debug)]
pub struct Error {
    message: String,
}

impl Error {
    pub fn new<T: ToString>(message: T) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

impl From<std::path::StripPrefixError> for Error {
    fn from(e: std::path::StripPrefixError) -> Self {
        Self::new(&format!("Error stripping prefix: {}", e))
    }
}

impl From<mpsc::RecvError> for Error {
    fn from(e: mpsc::RecvError) -> Self {
        Self::new(&format!("Error receiving from channel: {}", e))
    }
}

impl From<mpsc::TryRecvError> for Error {
    fn from(e: mpsc::TryRecvError) -> Self {
        Self::new(&format!("Error trying to receive from channel: {}", e))
    }
}

impl From<mpsc::SendError<(String, f32)>> for Error {
    fn from(e: mpsc::SendError<(String, f32)>) -> Self {
        Self::new(&format!("Error sending to channel: {}", e))
    }
}
