use crate::PathBufExt;
use crate::task::*;
use crate::wrappers::{CommandWrapper, RsyncCommand};
use anyhow::Result;
use console::style;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

const TASK_NAME: &str = "backup";

#[derive(Clone)]
pub struct BackupTask;

impl Task for BackupTask {
    fn name(&self) -> &str {
        "backup"
    }

    fn start_message(&self) -> &str {
        "backing up files..."
    }

    fn finish_message(&self, result: &TaskResult) -> String {
        let n = style(result.paths_modified.len()).bold();
        let p = style(result.task_backup_path.to_string()).bold();
        format!("{n} files backed up to {p}")
    }

    fn collect_paths(&self, params: &Params) -> Vec<PathBuf> {
        crate::collect_paths(&params.input_path)
    }

    fn spawn(&self, params: &Params) -> Result<TaskProcess> {
        Ok(TaskProcess::new(self, params))
    }

    fn thread_fn(&self, params: &Params) -> TaskFn {
        let name = self.name().to_string();
        let params = params.clone();
        let is_dry_run = params.is_dry_run;
        let input_path = params.input_path.clone();

        let func = move |mut handle: TaskProcessHandle| {
            handle.logv(format!("source path: {}", params.input_path.to_string()));
            handle.logv(format!("backup path: {}", params.backup_path.to_string()));
            handle.logv(format!("total files: {}", handle.paths().len()));

            handle.logv(
                style(format!("starting task \"{}\"...", name))
                    .green()
                    .to_string(),
            );

            let mut rsync_cmd = RsyncCommand::new(&input_path, handle.task_backup_path())?;

            handle.logv(format!("rsync: {}", rsync_cmd.program_path()));
            handle.logv(format!("{}", style(rsync_cmd.command_string()).blue()));

            let mut processed_paths = vec![];
            let mut num_processed = 0;

            if is_dry_run {
                processed_paths = handle.paths().clone();
                return Ok(handle.finished(processed_paths.clone(), processed_paths));
            }

            let rsync_process = rsync_cmd.run()?;
            let (stdout, stderr) = (&rsync_process.stdout, &rsync_process.stderr);

            loop {
                // if !handle.should_run() {
                //     handle.log("killing rsync");
                //     rsync_process.kill()?;
                //     return Ok(TaskResult::interrupted(
                //         processed_paths.clone(),
                //         processed_paths,
                //     ));
                // }

                match stdout.recv() {
                    Ok(path) => {
                        num_processed += 1;
                        processed_paths.push(Path::new(&path.clone()).to_path_buf());
                        let _ = handle.send_progress(
                            TASK_NAME,
                            path,
                            num_processed,
                            handle.paths().len(),
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

            std::thread::sleep(std::time::Duration::from_millis(500));

            if handle.should_run() {
                Ok(handle.finished(processed_paths.clone(), processed_paths))
            } else {
                Ok(handle.interrupted(processed_paths.clone(), processed_paths))
            }
        };
        Box::new(func)
    }
}
