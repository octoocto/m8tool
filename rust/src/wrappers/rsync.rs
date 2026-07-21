use crate::{which, wrappers::CommandWrapper};
use anyhow::Result;
use std::{path::PathBuf, process};

pub struct RsyncCommand {
    command: process::Command,
}

impl RsyncCommand {
    /// Create a new rsync command that will copy all files
    /// from `path_in` to `path_out`.
    ///
    /// Returns an [Error] if the `rsync` command does not exist
    /// on the system.
    pub fn new(path_in: &PathBuf, path_out: &PathBuf) -> Result<Self> {
        let rsync_path = which("rsync")?;
        let mut command = process::Command::new(rsync_path);

        command.stdin(process::Stdio::null());
        #[cfg(not(target_os = "windows"))]
        command.arg("-a");
        #[cfg(target_os = "windows")]
        command.arg("-rtD");
        command.arg("--out-format=%n").arg(&path_in).arg(&path_out);

        Ok(Self { command })
    }
}

impl CommandWrapper for RsyncCommand {
    fn command(&mut self) -> &mut process::Command {
        &mut self.command
    }
}
