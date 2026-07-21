use console::style;
use std::{path::PathBuf, sync::mpsc};

use crate::{Error, FileStatus, PathBufExt, generate_backup_name};

pub type TaskThread = std::thread::JoinHandle<Result<TaskResult, Error>>;

pub trait Task {
    fn name(&self) -> &str;

    fn params(&self) -> &dyn AsTaskParams;

    /// Get the paths to files and/or directories that will be processed by
    /// this task.
    ///
    /// Paths in this list could be deleted or modified by the task.
    fn paths(&self) -> &Vec<PathBuf>;

    /// Get the paths to files and/or directories that have been processed by
    /// this task.
    // fn processed_paths(&self) -> Vec<PathBuf>;

    /// Start the task. This will usually be non-blocking and spawn a child process.
    fn start(&mut self) -> Result<(), Error>;

    fn is_running(&mut self) -> bool;

    /// Kill the task. This will usually kill the child process.
    fn kill(&mut self) -> Result<(), Error>;

    /// Join the task. This will usually wait for the child process to finish.
    fn join(&mut self) -> Result<TaskResult, Error>;

    /// Get the backup path for this task.
    /// This will be a path to a new directory created in the backup directory.
    fn task_backup_path(&self) -> Option<PathBuf>;

    fn set_task_backup_path(&mut self, path: PathBuf);

    fn message_channel(&self) -> &(mpsc::Sender<TaskMessage>, mpsc::Receiver<TaskMessage>);

    fn is_verbose(&self) -> bool {
        self.params().is_verbose()
    }

    fn is_dry_run(&self) -> bool {
        self.params().is_dry_run()
    }

    // fn collect_params(&self) -> Result<String, Error> {
    //     let params = self.params();
    //     let message_tx = self.message_tx().clone();
    //     let task_backup_path = self.generate_backup_path()?;
    // }

    fn generate_backup_path(&mut self) -> Result<PathBuf, Error> {
        let backup_name = generate_backup_name(self.name());
        let backup_path = self
            .params()
            .backup_path()
            .join(&backup_name)
            .with_trailing_separator();
        backup_path.expect_not_exist()?;
        self.set_task_backup_path(backup_path.clone());
        self.log(format!(
            "{} {}",
            style("generated backup path:").green(),
            backup_path.to_string_lossy()
        ));
        Ok(backup_path)
    }

    fn message_tx(&self) -> &mpsc::Sender<TaskMessage> {
        &self.message_channel().0
    }

    fn message_rx(&self) -> &mpsc::Receiver<TaskMessage> {
        &self.message_channel().1
    }

    /// Receive all pending messages from this task.
    fn receive_messages(&mut self) -> Vec<TaskMessage> {
        let mut messages = Vec::new();
        loop {
            match self.message_rx().try_recv() {
                Ok(message) => messages.push(message),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(_) => break,
            }
        }
        messages
    }

    fn log(&mut self, message: String) {
        self.message_tx().log(message);
    }

    fn logv(&mut self, message: String) {
        if self.is_verbose() {
            self.log(message);
        }
    }
}

pub struct TaskResult {
    /// The list of paths that were processed by this task.
    pub paths: Vec<PathBuf>,

    /// The list of paths that were modified by this task.
    pub paths_modified: Vec<PathBuf>,
}

impl TaskResult {
    pub fn new(paths: Vec<PathBuf>, paths_modified: Vec<PathBuf>) -> Self {
        Self {
            paths,
            paths_modified,
        }
    }
}

#[derive(Clone)]
pub enum TaskMessage {
    Progress {
        task_name: String,
        file: String,
        count: usize,
        total: usize,
        file_status: FileStatus,
        metadata: Option<String>,
    },
    Log(String),
}

pub trait TaskMessageSender {
    fn log<T>(&self, message: T)
    where
        T: ToString;

    fn send_progress(&self, task_name: &str, file: String, count: usize, total: usize);

    fn send_progress_with_meta(
        &self,
        task_name: &str,
        file: String,
        count: usize,
        total: usize,
        file_status: FileStatus,
        metadata: String,
    );
}

fn ansi_to_bbcode(input: &str) -> String {
    input
        .replace("\x1b[30m", "[color=black]")
        .replace("\x1b[31m", "[color=red]")
        .replace("\x1b[32m", "[color=green]")
        .replace("\x1b[33m", "[color=yellow]")
        .replace("\x1b[34m", "[color=blue]")
        .replace("\x1b[0m", "[/color]")
}

impl TaskMessageSender for mpsc::Sender<TaskMessage> {
    fn log<T>(&self, message: T)
    where
        T: ToString,
    {
        if let Err(e) = self.send(TaskMessage::Log(message.to_string())) {
            eprintln!("Error sending message: {}", e);
        }
    }

    fn send_progress(&self, task_name: &str, file: String, count: usize, total: usize) {
        if let Err(e) = self.send(TaskMessage::Progress {
            task_name: task_name.to_string(),
            file,
            count,
            total,
            file_status: FileStatus::None,
            metadata: None,
        }) {
            eprintln!("Error sending progress message: {}", e);
        }
    }

    fn send_progress_with_meta(
        &self,
        task_name: &str,
        file: String,
        count: usize,
        total: usize,
        file_status: FileStatus,
        metadata: String,
    ) {
        if let Err(e) = self.send(TaskMessage::Progress {
            task_name: task_name.to_string(),
            file,
            count,
            total,
            file_status,
            metadata: Some(metadata),
        }) {
            eprintln!("Error sending progress message: {}", e);
        }
    }
}

pub trait AsTaskParams {
    /// Get the source directory for this task.
    fn source_path(&self) -> PathBuf;
    /// Get the backup directory for this task.
    fn backup_path(&self) -> PathBuf;
    fn is_dry_run(&self) -> bool;
    fn is_verbose(&self) -> bool;

    fn expect_params_are_valid(&self) -> Result<(), Error> {
        self.source_path().expect_dir_not_empty()?;
        self.backup_path().expect_dir()?;
        Ok(())
    }

    fn source_path_as_string(&self) -> String {
        self.source_path().to_string_lossy().to_string()
    }

    fn backup_path_as_string(&self) -> String {
        self.backup_path().to_string_lossy().to_string()
    }
}

#[derive(Clone)]
pub struct TaskParams {
    source_path: PathBuf,
    backup_path: PathBuf,
    is_dry_run: bool,
    is_verbose: bool,
}

impl TaskParams {
    pub fn new(
        source_path: PathBuf,
        backup_path: PathBuf,
        is_dry_run: bool,
        is_verbose: bool,
    ) -> Result<Self, Error> {
        let source_path = source_path.with_trailing_separator();
        let backup_path = backup_path.with_trailing_separator();
        let params = Self {
            source_path,
            backup_path,
            is_dry_run,
            is_verbose,
        };
        params.expect_params_are_valid()?;
        Ok(params)
    }
}

impl AsTaskParams for TaskParams {
    fn source_path(&self) -> PathBuf {
        self.source_path.clone()
    }

    fn backup_path(&self) -> PathBuf {
        self.backup_path.clone()
    }

    fn is_dry_run(&self) -> bool {
        self.is_dry_run
    }

    fn is_verbose(&self) -> bool {
        self.is_verbose
    }
}
