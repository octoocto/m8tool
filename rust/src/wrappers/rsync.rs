use crate::{Error, which, wrappers::CommandWrapper};
use std::{path::PathBuf, process};

pub struct RsyncCommand {
    command: process::Command,
    path_in: PathBuf,
    path_out: PathBuf,
}

impl RsyncCommand {
    /// Create a new rsync command that will copy all files
    /// from `path_in` to `path_out`.
    ///
    /// Returns an [Error] if the `rsync` command does not exist
    /// on the system.
    pub fn new(path_in: &PathBuf, path_out: &PathBuf) -> Result<Self, Error> {
        let rsync_path = which("rsync")?;
        let mut command = process::Command::new(rsync_path);

        #[cfg(not(target_os = "windows"))]
        command.arg("-a");
        #[cfg(target_os = "windows")]
        command.arg("-rtD");
        command.arg("--out-format=%n").arg(&path_in).arg(&path_out);

        Ok(Self {
            command,
            path_in: path_in.clone(),
            path_out: path_out.clone(),
        })
    }
}

impl CommandWrapper for RsyncCommand {
    fn command(&mut self) -> &mut process::Command {
        &mut self.command
    }
}
