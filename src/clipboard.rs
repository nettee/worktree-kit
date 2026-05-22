use crate::{AppResult, Error};
use std::io::Write;
use std::process::{Command, Stdio};

pub trait ClipboardProvider {
    fn write_text(&mut self, value: &str) -> AppResult<()>;
}

#[derive(Default)]
pub struct DisabledClipboard;

impl ClipboardProvider for DisabledClipboard {
    fn write_text(&mut self, _value: &str) -> AppResult<()> {
        Ok(())
    }
}

#[derive(Default)]
pub struct SystemClipboard;

impl ClipboardProvider for SystemClipboard {
    fn write_text(&mut self, value: &str) -> AppResult<()> {
        let (program, args): (&str, Vec<&str>) = match std::env::consts::OS {
            "macos" => ("pbcopy", Vec::new()),
            "windows" => ("clip", Vec::new()),
            "linux" => {
                if command_exists("wl-copy") {
                    ("wl-copy", Vec::new())
                } else if command_exists("xclip") {
                    ("xclip", vec!["-selection", "clipboard"])
                } else if command_exists("xsel") {
                    ("xsel", vec!["--clipboard", "--input"])
                } else {
                    return Err(Error::message(
                        "missing clipboard command: install wl-copy, xclip, or xsel",
                    ));
                }
            }
            other => {
                return Err(Error::message(format!(
                    "unsupported clipboard platform: {other}"
                )));
            }
        };

        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|error| {
                Error::message(format!(
                    "failed to start clipboard command {program}: {error}"
                ))
            })?;

        {
            let stdin = child.stdin.as_mut().ok_or_else(|| {
                Error::message(format!("clipboard command {program} did not open stdin"))
            })?;
            stdin.write_all(value.as_bytes()).map_err(|error| {
                Error::message(format!(
                    "failed to write clipboard data to {program}: {error}"
                ))
            })?;
        }

        let status = child.wait().map_err(|error| {
            Error::message(format!(
                "failed to wait for clipboard command {program}: {error}"
            ))
        })?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::message(format!(
                "clipboard command {program} exited with status {status}"
            )))
        }
    }
}

fn command_exists(program: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {program} >/dev/null 2>&1"))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
