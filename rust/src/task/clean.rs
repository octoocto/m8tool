use crate::PathBufExt;
use crate::remove_and_backup_dir;
use crate::remove_and_backup_file;
use crate::task::*;
use anyhow::Result;
use console::style;
use std::path::PathBuf;

const VALID_EXTENSIONS: &[&str] = &["m8t", "m8s", "m8i", "m8n", "wav"];

const DIR_BLACKLIST: &[&str] = &["__MACOSX", ".DS_Store", "System Volume Information"];

#[derive(Clone)]
pub struct CleanTask;

impl Task for CleanTask {
    fn name(&self) -> &str {
        "clean"
    }

    fn start_message(&self) -> &str {
        "cleaning up extra files..."
    }

    fn finish_message(&self, result: &TaskResult) -> String {
        let n = style(result.paths_modified.len()).bold();
        let p = style(result.task_backup_path.to_string()).bold();
        format!("{n} files cleaned, and backed up to {p}")
    }

    fn collect_paths(&self, params: &Params) -> Vec<PathBuf> {
        collect_paths_to_clean(&params.input_path)
    }

    fn spawn(&self, params: &Params) -> Result<TaskProcess> {
        Ok(TaskProcess::new(self, params))
    }

    fn thread_fn(&self, params: &Params) -> TaskFn {
        let source_path = params.input_path.clone();
        let is_dry_run = params.is_dry_run;
        let name = self.name().to_owned();

        let func = move |mut handle: TaskProcessHandle| {
            let mut num_removed_files = 0;
            let mut num_removed_dirs = 0;

            let total_paths = handle.paths().len();
            let mut processed_paths = vec![];

            let source_path = source_path.clone();

            handle.logv(
                style(format!("starting task \"{name}\"..."))
                    .green()
                    .to_string(),
            );

            for path in handle.paths().clone() {
                if !handle.should_run() {
                    return Ok(handle.interrupted(processed_paths.clone(), processed_paths));
                }
                let path_relative = if path.is_dir() {
                    path.strip_prefix(&source_path)?
                        .to_path_buf()
                        .with_trailing_separator()
                } else {
                    path.strip_prefix(&source_path)?.to_path_buf()
                };

                handle.logv(format!("{}", path_relative.display()));

                let backup_path = handle.task_backup_path().join(&path_relative);
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

                handle.send_progress(
                    &name,
                    path_relative.to_string_lossy().to_string(),
                    processed_paths.len(),
                    total_paths,
                );
            }

            handle.logv(
                style(format!(
                    "cleaned {} extra file(s) and {} extra dir(s)",
                    num_removed_files, num_removed_dirs
                ))
                .green(),
            );
            handle.logv(
                style(format!(
                    "backup of cleaned files has been made in: {}",
                    handle.task_backup_path().display()
                ))
                .green(),
            );

            Ok(handle.interrupted(processed_paths.clone(), processed_paths))
        };
        Box::new(func)
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
