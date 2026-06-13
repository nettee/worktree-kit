pub mod auxiliary;
pub mod cli;
pub mod clipboard;
pub mod gitexec;
pub mod list;
pub mod output;
pub mod paths;
pub mod upgrade;
pub mod worktree;

pub const IS_RELEASE_BUILD: bool = option_env!("WTK_VERSION").is_some();
pub const VERSION: &str = match option_env!("WTK_VERSION") {
    Some(version) => version,
    None => "0.0.1",
};

pub type AppResult<T> = Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Message(String),
    Io(std::io::Error),
    Git(gitexec::GitError),
}

impl Error {
    pub fn message(message: impl Into<String>) -> Error {
        Error::Message(message.into())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Message(message) => write!(f, "{message}"),
            Error::Io(error) => write!(f, "{error}"),
            Error::Git(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Message(_) => None,
            Error::Io(error) => Some(error),
            Error::Git(error) => Some(error),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error::Io(value)
    }
}

impl From<gitexec::GitError> for Error {
    fn from(value: gitexec::GitError) -> Self {
        Error::Git(value)
    }
}

impl From<String> for Error {
    fn from(value: String) -> Self {
        Error::Message(value)
    }
}

impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Error::Message(value.to_string())
    }
}
