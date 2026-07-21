use crate::tasks::*;
use crate::wrappers::{CommandWrapper, RsyncCommand};
use crate::{Error, PathBufExt};
use console::style;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::mpsc;

const TASK_NAME: &str = "backup";

pub struct BackupTask {
    params: TaskParams,
    task_backup_path: Option<PathBuf>,
    message_channel: (
        std::sync::mpsc::Sender<TaskMessage>,
        std::sync::mpsc::Receiver<TaskMessage>,
    ),
    reader_thread: Option<TaskThread>,
    // A vector of paths to files that will be backed up.
    paths: Vec<PathBuf>,
    // The child process for the rsync command
    child: Option<process::Child>,
    // If the child process (rsync) has finished, contains its exit status
    exit_status: Option<process::ExitStatus>,
}

impl BackupTask {
    pub fn new(
        source_dir: PathBuf,
        backup_dir: PathBuf,
        is_dry_run: bool,
        is_verbose: bool,
    ) -> Result<Self, Error> {
        let params = TaskParams::new(source_dir, backup_dir, is_dry_run, is_verbose)?;
        Self::from_params(params)
    }

    pub fn from_params(params: TaskParams) -> Result<Self, Error> {
        params.expect_params_are_valid()?;
        Ok(Self {
            params,
            task_backup_path: None,
            message_channel: mpsc::channel(),
            reader_thread: None,
            paths: Vec::new(),
            child: None,
            exit_status: None,
        })
    }
}

impl Task for BackupTask {
    fn name(&self) -> &str {
        TASK_NAME
    }

    fn params(&self) -> &dyn AsTaskParams {
        &self.params
    }

    fn paths(&self) -> &Vec<PathBuf> {
        &self.paths
    }

    fn start(&mut self) -> Result<(), Error> {
        let params = self.params();
        let source_path = params.source_path();
        let message_tx = self.message_tx().clone();
        let task_backup_path = self.generate_backup_path()?;

        if self.is_verbose() {
            self.log(
                style(format!("starting task \"{}\"...", self.name()))
                    .green()
                    .to_string(),
            );
        }

        // get total files

        self.paths = crate::collect_paths(&source_path);
        let paths = self.paths.clone();

        self.logv(format!("source path: {}", source_path.to_string()));
        self.logv(format!("backup path: {}", task_backup_path.to_string()));
        self.logv(format!("total files: {}", paths.len()));

        // run rsync

        let mut rsync_cmd = RsyncCommand::new(&source_path, &task_backup_path)?;

        self.logv(format!("rsync: {}", rsync_cmd.program_path()));
        self.logv(format!("{}", style(rsync_cmd.command_string()).blue()));

        let (rsync_child, rsync_recv) = if !self.is_dry_run() {
            let process = rsync_cmd.run()?;
            (Some(process.child), Some((process.stdout, process.stderr)))
        } else {
            (None, None)
        };

        self.child = rsync_child;

        // setup reader thread

        self.reader_thread = Some(std::thread::spawn(move || {
            let mut processed_paths = vec![];
            let mut num_processed = 0;
            if let Some((stdout, stderr)) = &rsync_recv {
                loop {
                    match stdout.recv() {
                        Ok(path) => {
                            num_processed += 1;
                            processed_paths.push(Path::new(&path.clone()).to_path_buf());
                            let _ = message_tx.send_progress(
                                TASK_NAME,
                                path,
                                num_processed,
                                paths.len(),
                            );
                        }
                        Err(_) => {
                            println!();
                            break;
                        }
                    }
                    match stderr.try_recv() {
                        Ok(line) => {
                            eprintln!("{}", line);
                        }
                        Err(mpsc::TryRecvError::Empty) => {}
                        Err(_) => {
                            break;
                        }
                    }
                }
            } else {
                // dry run case
                processed_paths = paths.clone();
            }

            Ok(TaskResult::new(processed_paths.clone(), processed_paths))
        }));

        Ok(())
    }

    fn is_running(&mut self) -> bool {
        let child = match self.child.as_mut() {
            Some(child) => child,
            None => return false,
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    eprintln!("rsync failed with status: {}", status);
                    return false;
                } else {
                    if self.params.is_verbose() {
                        println!("{}", style("backup completed successfully").green());
                    }
                    false
                }
            }
            Ok(None) => true,
            Err(e) => {
                eprintln!("Failed to wait for rsync process: {}", e);
                return false;
            }
        }
    }

    fn kill(&mut self) -> Result<(), Error> {
        let child = match self.child.as_mut() {
            Some(child) => child,
            None => return Err(Error::new("backup process not started")),
        };
        child.kill().map_err(|e| {
            eprintln!("Failed to kill rsync process: {}", e);
            e.into()
        })
    }

    fn join(&mut self) -> Result<TaskResult, Error> {
        if !self.params.is_dry_run() {
            self.wait()?;
        }
        if let Some(reader_thread) = self.reader_thread.take() {
            reader_thread
                .join()
                .map_err(|e| Error::new(format!("error joining reader thread: {:?}", e)))
                .flatten()
        } else {
            Err(Error::new("reader thread not started"))
        }
    }

    fn task_backup_path(&self) -> Option<PathBuf> {
        self.task_backup_path.clone()
    }

    fn set_task_backup_path(&mut self, path: PathBuf) {
        self.task_backup_path = Some(path);
    }

    fn message_channel(&self) -> &(mpsc::Sender<TaskMessage>, mpsc::Receiver<TaskMessage>) {
        &self.message_channel
    }
}

impl BackupTask {
    /// Wait for the child process (rsync) to finish, returning its exit status.
    fn wait(&mut self) -> Result<(), Error> {
        let exit_status;
        if let Some(mut child) = self.child.take() {
            exit_status = if let Some(status) = child.try_wait()? {
                status
            } else {
                child
                    .wait()
                    .map_err(|e| Error::new(format!("failed to wait for backup process: {}", e)))?
            };
        } else {
            return Err(Error::new("could not get exit status from process"));
        }
        self.exit_status = Some(exit_status);
        Ok(())
    }

    pub fn exit_status(&self) -> &Option<process::ExitStatus> {
        &self.exit_status
    }
}
