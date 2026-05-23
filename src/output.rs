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
    command(out, dir, "git", args)
}

pub fn command(out: &mut dyn Write, dir: &Path, program: &str, args: &[String]) -> io::Result<()> {
    writeln!(out, "$ {program} -C {} {}", dir.display(), args.join(" "))
}
