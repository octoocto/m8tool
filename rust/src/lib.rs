pub mod audio;
#[cfg(feature = "gdext")]
mod gdext;
pub mod task;
pub mod wrappers;

pub use task::*;

use anyhow::{Error, Result, anyhow, bail, ensure};
use chrono::prelude::*;
use console::style;
use std::path::{MAIN_SEPARATOR_STR, Path, PathBuf};
use sysinfo::Disks;
use walkdir::WalkDir;

const TIME_FORMAT: &str = "%Y-%m-%d-%H%M%S";

pub trait PathBufExt {
    fn to_string(&self) -> String;
    fn with_trailing_separator(&self) -> PathBuf;
    fn has_extension(&self, extension: &str) -> bool;
    fn get_extension(&self) -> Option<String>;

    fn expect_exists(&self) -> Result<()>;
    fn expect_not_exist(&self) -> Result<()>;
    fn expect_dir(&self) -> Result<()>;
    fn expect_file(&self) -> Result<()>;
    fn expect_mount_point(&self) -> Result<()>;
    fn expect_dir_not_empty(&self) -> Result<()>;
    fn expect_relative(&self) -> Result<()>;
    fn expect_extension(&self, extension: &str) -> Result<()>;

    fn base_dir(&self) -> Result<&Path>;
    fn file_or_dir_name(&self) -> Result<String>;
    fn file_stem(&self) -> Result<String>;

    fn is_dotfile(&self) -> bool;
    fn is_wav_file(&self) -> bool {
        self.has_extension("wav")
    }
}

impl PathBufExt for PathBuf {
    fn to_string(&self) -> String {
        self.to_string_lossy().to_string()
    }

    fn with_trailing_separator(&self) -> PathBuf {
        if self.as_os_str().is_empty() {
            return self.clone();
        }
        let mut path = self.clone();
        if !path
            .as_os_str()
            .to_string_lossy()
            .ends_with(MAIN_SEPARATOR_STR)
        {
            path.push("");
        }
        path
    }

    fn has_extension(&self, extension: &str) -> bool {
        self.extension()
            .and_then(|e| e.to_str().map(|s| s.to_lowercase()))
            == Some(extension.to_lowercase())
    }

    fn get_extension(&self) -> Option<String> {
        if !self.is_file() {
            return None;
        }
        match self.extension() {
            Some(ext) => ext.to_str().map(|s| s.to_string()),
            None => self.file_or_dir_name().ok(),
        }
    }

    fn expect_exists(&self) -> Result<()> {
        ensure!(self.is_dir(), "path does not exist: {}", self.display());
        Ok(())
    }

    fn expect_not_exist(&self) -> Result<()> {
        ensure!(!self.exists(), "path already exists: {}", self.display());
        Ok(())
    }
    fn expect_dir(&self) -> Result<()> {
        ensure!(
            self.is_dir(),
            "path does not exist or is not a directory: {}",
            self.display()
        );
        Ok(())
    }
    fn expect_file(&self) -> Result<()> {
        ensure!(
            self.is_file(),
            "path does not exist or is not a file: {}",
            self.display()
        );
        Ok(())
    }
    fn expect_dir_not_empty(&self) -> Result<()> {
        if self.is_dir() {
            let mut entries = std::fs::read_dir(self)?;
            if entries.next().is_some() {
                Ok(())
            } else {
                bail!("directory is empty: {}", self.display());
            }
        } else {
            bail!(
                "path does not exist or is not a directory: {}",
                self.display()
            );
        }
    }
    // Checks if the source path is the root path of a disk.
    fn expect_mount_point(&self) -> Result<(), Error> {
        let disk_mount_points = sysinfo::Disks::new_with_refreshed_list()
            .list()
            .iter()
            // filter out disks smaller than 16GB and larger than 1TB
            // .filter(|d| (d.total_space() as f64) >= 1.6e10 && (d.total_space() as f64) < 1.0e12)
            .map(|d| d.mount_point().to_string_lossy().to_string())
            .collect::<Vec<String>>();

        if disk_mount_points.contains(&self.to_string_lossy().to_string()) {
            Ok(())
        } else {
            bail!("path is not the root of a disk: {}", self.display());
        }
    }

    fn expect_relative(&self) -> Result<(), Error> {
        if self.is_relative() {
            Ok(())
        } else {
            bail!("path is not relative: {}", self.display());
        }
    }

    fn expect_extension(&self, extension: &str) -> Result<(), Error> {
        if self.has_extension(extension) {
            Ok(())
        } else {
            bail!(
                "path does not have the expected extension '{}': {}",
                extension,
                self.display()
            );
        }
    }

    fn base_dir(&self) -> Result<&Path, Error> {
        self.parent()
            .ok_or_else(|| anyhow!("could not get base directory from path: {}", self.display()))
    }

    fn file_or_dir_name(&self) -> Result<String, Error> {
        self.file_name()
            .and_then(|str| str.to_str().map(|s| s.to_string()))
            .ok_or_else(|| anyhow!("could not get file name from path: {}", self.display()))
    }

    fn file_stem(&self) -> Result<String, Error> {
        Path::file_stem(self)
            .and_then(|os_str| os_str.to_str())
            .and_then(|s| Some(s.to_string()))
            .ok_or_else(|| anyhow!("could not get file stem from path: {}", self.display()))
    }

    fn is_dotfile(&self) -> bool {
        self.file_or_dir_name()
            .map(|name| name.starts_with('.'))
            .unwrap_or(false)
    }
}

// Represents the status of a file after processing.
#[derive(Clone)]
pub enum FileStatus {
    Unchanged,
    // File was modified
    Changed,
    // Processing the file was skipped with a reason for skipping
    Skipped(String),
    // File was renamed with the new name provided
    Renamed(String),
    // File was removed
    Removed,
}

impl std::fmt::Display for FileStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileStatus::Unchanged => f.write_str(&"unchanged"),
            FileStatus::Changed => f.write_str(&"changed"),
            FileStatus::Skipped(_) => f.write_str(&"skipped"),
            FileStatus::Removed => f.write_str(&"removed"),
            FileStatus::Renamed(_) => f.write_str(&"renamed"),
        }
    }
}

/// Generate a name for a backup folder.
/// The name is dependant on the current time.
pub fn generate_backup_name(prefix: &str) -> String {
    let now = Utc::now();
    format!("{}-{}", prefix, now.format(TIME_FORMAT).to_string())
}

pub fn which(command: &str) -> Result<PathBuf> {
    which::which(command).map_err(|_| {
        anyhow!("'{command}' command not found. Please install {command} and try again.")
    })
}

/// Moves a file from the source to the backup location, creating any
/// necessary directories in the backup path.
fn remove_and_backup_file(source: &Path, backup: &Path) -> Result<()> {
    std::fs::create_dir_all(
        backup
            .parent()
            .ok_or(anyhow!("Failed to get backup path parent."))?,
    )?;
    std::fs::copy(source, backup)?;
    std::fs::remove_file(source)?;
    Ok(())
}

/// Moves a directory from the source to the backup location, creating any
fn remove_and_backup_dir(source: &Path, backup: &Path) -> Result<()> {
    std::fs::create_dir_all(backup.parent().unwrap())?;

    let source_dir = PathBuf::from(&source).with_trailing_separator();
    let backup_dir = PathBuf::from(&backup).with_trailing_separator();
    println!(
        "rsyncing from {} to {}",
        source_dir.display(),
        backup_dir.display()
    );

    let mut rsync_command = RsyncCommand::new(&source_dir, &backup_dir)?;
    let status = rsync_command.run_and_wait()?;
    status
        .success()
        .then(|| ())
        .ok_or(anyhow!("Rsync command failed."))?;

    std::fs::remove_dir_all(source)?;
    Ok(())
}

use std::process::{self, Child};
use std::sync::mpsc;

use crate::wrappers::{CommandWrapper, RsyncCommand};

fn command_run(
    command: &mut process::Command,
    dry_run: bool,
    print_command: bool,
) -> Result<Option<process::Output>, Error> {
    if print_command {
        println!("{}", style(command_to_string(&command)).blue());
    }
    if dry_run {
        return Ok(None);
    }
    match command.output() {
        Ok(output) => {
            if !output.status.success() {
                eprintln!("{}", String::from_utf8_lossy(&output.stderr));
                bail!(
                    "{} failed with status: {}",
                    command.get_program().to_string_lossy(),
                    output.status
                );
            }
            Ok(Some(output))
        }
        Err(e) => Err(e.into()),
    }
}

fn command_spawn(
    command: &mut std::process::Command,
    is_dry_run: bool,
    print_command: bool,
) -> Result<Option<(mpsc::Receiver<String>, mpsc::Receiver<String>, Child)>, Error> {
    if print_command {
        println!("{}", style(command_to_string(&command)).blue());
    }
    if is_dry_run {
        return Ok(None);
    }
    let mut child = command
        .stdin(process::Stdio::null())
        .stdout(process::Stdio::piped())
        .stderr(process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn command: {}", e))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Failed to capture stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("Failed to capture stderr"))?;

    let (stdout_tx, stdout_rx) = mpsc::channel();
    let (stderr_tx, stderr_rx) = mpsc::channel();

    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader};
        let stdout_reader = BufReader::new(stdout);
        let stderr_reader = BufReader::new(stderr);
        for line in stdout_reader.lines() {
            if let Ok(line) = line {
                if let Err(e) = stdout_tx.send(line) {
                    eprintln!("Error sending stdout line: {}", e);
                    break;
                }
            }
        }
        for line in stderr_reader.lines() {
            if let Ok(line) = line {
                if let Err(e) = stderr_tx.send(line) {
                    eprintln!("Error sending stderr line: {}", e);
                    break;
                }
            }
        }
    });

    Ok(Some((stdout_rx, stderr_rx, child)))
}

fn command_to_string(command: &std::process::Command) -> String {
    let mut cmd_str = String::new();
    cmd_str.push_str(&format!(
        "{} ",
        Path::new(command.get_program())
            .file_name()
            .unwrap()
            .to_string_lossy()
    ));
    for arg in command.get_args() {
        if arg.to_string_lossy().contains(' ') {
            cmd_str.push_str(&format!("\"{}\" ", arg.to_string_lossy()));
        } else {
            cmd_str.push_str(&format!("{} ", arg.to_string_lossy()));
        }
    }
    cmd_str.trim_end().to_string()
}

fn is_path_in_dir_whitelist(path: &Path, whitelisted_dirs: &Vec<String>) -> Result<bool, Error> {
    path.to_path_buf().expect_relative()?;
    let mut components = path
        .components()
        .map(|c| c.as_os_str().to_str().unwrap_or(""));
    Ok(components.next().is_some_and(|c| {
        whitelisted_dirs
            .iter()
            .any(|s| c.to_lowercase() == s.to_lowercase())
    }))
}

// fn is_handle_running<T>(handle: &Option<JoinHandle<T>>) -> Result<bool, Error> {
//     if let Some(handle) = handle {
//         Ok(!handle.is_finished())
//     } else {
//         Err(Error::new("task not started"))
//     }
// }

// fn kill_handle<T>(
//     handle: &mut Option<JoinHandle<T>>,
//     atomic_flag: &mut Arc<AtomicBool>,
// ) -> Result<(), Error> {
//     atomic_flag.store(false, std::sync::atomic::Ordering::Relaxed);
//     if let Some(handle) = handle.take() {
//         handle
//             .join()
//             .map_err(|_| Error::new("failed to join thread"))?;
//     }
//     Ok(())
// }

/// Collects all paths in the input directory, including files and directories.
///
/// All of the paths returned are guaranteed to exist at the time of collection.
fn collect_paths(input_path: &PathBuf) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for entry in WalkDir::new(input_path) {
        if let Ok(entry) = entry.as_ref()
            && entry.path().exists()
        {
            let path = entry.path();
            paths.push(path.to_path_buf());
        }
    }
    paths
}

/// Try to find the mount point of an M8 SD card.
pub fn find_m8_sd_card_mount_points() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let disks = Disks::new_with_refreshed_list();
    for disk in &disks {
        if disk.is_removable()
            && (disk.total_space() as f64) >= 1.6e10 // 16 GB
            && (disk.total_space() as f64) < 1.0e12 // 1 TB
            && (disk.name().to_string_lossy().to_lowercase() == "m8"
                || disk
                    .mount_point()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_lowercase()
                    == "m8")
        {
            paths.push(disk.mount_point().to_path_buf());
        }
    }
    paths
}
