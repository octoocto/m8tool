pub mod fatsort;
pub mod rsync;

use std::{
    ops::{Deref, DerefMut},
    process,
    sync::mpsc,
};

pub use fatsort::FatsortCommand;
pub use rsync::RsyncCommand;

use crate::command_spawn;
use anyhow::{Result, anyhow, bail};

pub struct CommandProcess {
    pub stdout: mpsc::Receiver<String>,
    pub stderr: mpsc::Receiver<String>,
    pub child: process::Child,
}

impl Deref for CommandProcess {
    type Target = process::Child;

    fn deref(&self) -> &Self::Target {
        &self.child
    }
}

impl DerefMut for CommandProcess {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}

pub trait CommandWrapper {
    fn command(&mut self) -> &mut process::Command;

    fn command_string(&mut self) -> String {
        crate::command_to_string(self.command())
    }

    fn program_path(&mut self) -> String {
        self.command().get_program().to_string_lossy().to_string()
    }

    /// Runs this command in another thread and returns a tuple with:
    /// - the [std::process::Child] of the running command
    /// - the [mpsc::Receiver<String>] for the stdout of the command
    /// - the [mpsc::Receiver<String>] for the stderr of the command
    fn run(&mut self) -> Result<CommandProcess> {
        let command = self.command();
        if let Some((stdout, stderr, child)) = command_spawn(command, false, false)? {
            Ok(CommandProcess {
                child,
                stdout,
                stderr,
            })
        } else {
            bail!("could not run rsync command: {}", self.command_string());
        }
    }

    /// Runs this command and waits for it to finish, returning the exit status.
    fn run_and_wait(&mut self) -> Result<std::process::ExitStatus> {
        self.command().status().map_err(|e| {
            anyhow!(
                "failed to run rsync command: {}: {}",
                self.command_string(),
                e
            )
        })
    }
}
