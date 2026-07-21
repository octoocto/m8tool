use crate::Error;
use crate::PathBufExt;
use crate::is_handle_running;
use crate::kill_handle;
use crate::remove_and_backup_dir;
use crate::remove_and_backup_file;
use crate::tasks::*;
use console::style;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::thread;

const VALID_EXTENSIONS: &[&str] = &["m8t", "m8s", "m8i", "m8n", "wav"];

const DIR_BLACKLIST: &[&str] = &["__MACOSX", ".DS_Store", "System Volume Information"];

const TASK_NAME: &str = "clean";

pub struct CleanTask {
    params: TaskParams,
    // The unique backup path for this task, which is generated when the task starts.
    task_backup_path: Option<PathBuf>,
    message_channel: (
        std::sync::mpsc::Sender<TaskMessage>,
        std::sync::mpsc::Receiver<TaskMessage>,
    ),
    handle: Option<TaskThread>,
    is_running: Arc<AtomicBool>,
    paths: Vec<PathBuf>,
}

impl CleanTask {
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
            message_channel: mpsc::channel(),
            handle: None,
            paths: Vec::new(),
            task_backup_path: None,
            is_running: Arc::new(AtomicBool::new(false)),
        })
    }
}

impl Task for CleanTask {
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
        let source_path = self.params.source_path();
        let backup_path = self.params.backup_path();
        let is_dry_run = self.params.is_dry_run();
        let is_verbose = self.params.is_verbose();

        let message_tx = self.message_tx().clone();
        let task_backup_path = self.generate_backup_path()?;

        source_path.expect_dir_not_empty()?;
        backup_path.expect_dir()?;

        self.logv(
            style(format!("starting task \"{}\"...", self.name()))
                .green()
                .to_string(),
        );

        self.paths = collect_paths_to_clean(&source_path);
        self.is_running = Arc::new(AtomicBool::new(true));

        let paths = self.paths.clone();
        let is_running = self.is_running.clone();
        let handle = thread::spawn(move || {
            let result = CleanTask::run_task(
                &paths,
                &source_path,
                &task_backup_path,
                message_tx.clone(),
                &is_running,
                is_dry_run,
                is_verbose,
            );
            result
        });
        self.handle = Some(handle);

        Ok(())
    }

    fn is_running(&mut self) -> bool {
        is_handle_running(&self.handle).unwrap_or(false)
    }

    fn kill(&mut self) -> Result<(), Error> {
        kill_handle(&mut self.handle, &mut self.is_running)
    }

    fn join(&mut self) -> Result<TaskResult, Error> {
        if let Some(handle) = self.handle.take() {
            handle
                .join()
                .map_err(|e| Error::new(format!("error joining reader thread: {:?}", e)))
                .flatten()
        } else {
            Err(Error::new("task handle not started"))
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

impl CleanTask {
    fn run_task(
        paths: &Vec<PathBuf>,
        source_path: &PathBuf,
        task_backup_path: &PathBuf,
        message_tx: mpsc::Sender<TaskMessage>,
        is_running: &Arc<AtomicBool>,
        is_dry_run: bool,
        is_verbose: bool,
    ) -> Result<TaskResult, Error> {
        let mut num_removed_files = 0;
        let mut num_removed_dirs = 0;

        let total_paths = paths.len();
        let mut processed_paths = vec![];

        let source_path = source_path.clone();
        let task_backup_path = task_backup_path.clone();
        let is_running = is_running.clone();

        for path in paths {
            if !is_running.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            let path_relative = if path.is_dir() {
                path.strip_prefix(&source_path)?
                    .to_path_buf()
                    .with_trailing_separator()
            } else {
                path.strip_prefix(&source_path)?.to_path_buf()
            };

            if is_verbose {
                message_tx.log(format!("{}", path_relative.display()));
            }

            let backup_path = task_backup_path.join(&path_relative);
            if path.is_file() {
                if !is_dry_run {
                    remove_and_backup_file(&path, &backup_path)?;
                }
                num_removed_files += 1;
            } else {
                if !is_dry_run {
                    remove_and_backup_dir(&path, &backup_path)?;
                }
                num_removed_dirs += 1;
            }

            processed_paths.push(path.clone());

            message_tx.send_progress(
                TASK_NAME,
                path_relative.to_string_lossy().to_string(),
                processed_paths.len(),
                total_paths,
            );
        }

        if is_verbose {
            message_tx.log(
                style(format!(
                    "cleaned {} extra file(s) and {} extra dir(s)",
                    num_removed_files, num_removed_dirs
                ))
                .green(),
            );
            message_tx.log(
                style(format!(
                    "backup of cleaned files has been made in: {}",
                    task_backup_path.display()
                ))
                .green(),
            );
        }

        Ok(TaskResult::new(processed_paths.clone(), processed_paths))
    }
}

fn collect_paths_to_clean(source_path: &PathBuf) -> Vec<PathBuf> {
    // first pass: collect invalid files

    // (path, extension)
    let paths_to_remove: Vec<PathBuf> = crate::collect_paths(source_path)
        .into_iter()
        .filter_map(|path| {
            if path.is_file()
                && let Some(file_name) = path.file_or_dir_name().ok()
                && let Some(file_ext) = path.get_extension()
            {
                // remove if extension is invalid or if it's a dotfile
                let is_ext_valid = VALID_EXTENSIONS
                    .iter()
                    .any(|e| e.to_lowercase() == file_ext.to_lowercase());
                let is_dotfile = file_name.starts_with('.');
                if !is_ext_valid || is_dotfile {
                    Some(path.to_owned())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    // second pass: collect directories
    let dirs_to_check: Vec<PathBuf> = crate::collect_paths(&source_path)
        .into_iter()
        .filter_map(|path| {
            if path.is_dir()
                && let Some(file_name) = path.file_or_dir_name().ok()
            {
                let is_blacklisted = DIR_BLACKLIST.contains(&file_name.as_str());
                let is_dotfolder = file_name.starts_with('.');
                if is_blacklisted || is_dotfolder {
                    Some(path.to_owned().with_trailing_separator())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    paths_to_remove
        .into_iter()
        .chain(dirs_to_check.into_iter())
        .collect()
}
