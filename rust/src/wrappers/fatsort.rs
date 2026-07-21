use crate::{Error, PathBufExt, which, wrappers::CommandWrapper};
use std::{path::PathBuf, process};

pub struct FatsortCommand {
    command: process::Command,
    path_in: PathBuf,
}

impl FatsortCommand {
    /// Create a new rsync command that will sort all files
    /// in `path_in`.
    ///
    /// Returns an [Error] if:
    /// - the `fatsort` command does not exist on the system.
    /// - the `path_in` does not exist or is not a mount point.
    pub fn new(path_in: &PathBuf) -> Result<Self, Error> {
        path_in.expect_mount_point()?;

        let command_path = which("fatsort")?;
        let mut command = process::Command::new(command_path);

        command.arg("-f").arg("-n").arg("-c");
        command.arg(&path_in);

        Ok(Self {
            command,
            path_in: path_in.clone(),
        })
    }
}

impl CommandWrapper for FatsortCommand {
    fn command(&mut self) -> &mut process::Command {
        &mut self.command
    }
}
