use std::io::{self, Write};
use std::path::Path;

pub fn info(out: &mut dyn Write, message: &str) -> io::Result<()> {
    writeln!(out, "==> {message}")
}

pub fn success(out: &mut dyn Write, message: &str) -> io::Result<()> {
    writeln!(out, "✓ {message}")
}

pub fn warn(out: &mut dyn Write, message: &str) -> io::Result<()> {
    writeln!(out, "! {message}")
}

pub fn git(out: &mut dyn Write, dir: &Path, args: &[String]) -> io::Result<()> {
    writeln!(out, "$ git -C {} {}", dir.display(), args.join(" "))
}

#[derive(Debug, Clone, Copy)]
pub struct Style {
    enabled: bool,
}

impl Style {
    pub fn new(enabled: bool) -> Style {
        Style { enabled }
    }

    pub fn plain() -> Style {
        Style { enabled: false }
    }

    pub fn header<'a>(&self, text: &'a str) -> Styled<'a> {
        Styled::new(self.enabled, "\x1b[1m", text)
    }

    pub fn current<'a>(&self, text: &'a str) -> Styled<'a> {
        Styled::new(self.enabled, "\x1b[1;36m", text)
    }

    pub fn warning<'a>(&self, text: &'a str) -> Styled<'a> {
        Styled::new(self.enabled, "\x1b[33m", text)
    }

    pub fn error<'a>(&self, text: &'a str) -> Styled<'a> {
        Styled::new(self.enabled, "\x1b[31m", text)
    }
}

pub struct Styled<'a> {
    enabled: bool,
    code: &'static str,
    text: &'a str,
}

impl<'a> Styled<'a> {
    fn new(enabled: bool, code: &'static str, text: &'a str) -> Styled<'a> {
        Styled {
            enabled,
            code,
            text,
        }
    }
}

impl std::fmt::Display for Styled<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.enabled {
            write!(f, "{}{}\x1b[0m", self.code, self.text)
        } else {
            write!(f, "{}", self.text)
        }
    }
}
