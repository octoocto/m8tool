use crate::task;
use crate::{FileStatus, ParamsBuilder, Task, TaskMessage, TaskProcess};
use anyhow::{Error, Result};
use godot::{global::godot_str, prelude::*};
use sysinfo::{Disk, Disks};

type GError = godot::global::Error;

struct M8ToolExtension;

#[gdextension]
unsafe impl ExtensionLibrary for M8ToolExtension {}

#[derive(GodotClass)]
#[class(no_init)]
struct M8Tool;

#[godot_api]
impl M8Tool {
    fn get_disk_list(disks: &Disks) -> Vec<&Disk> {
        disks
            .list()
            .iter()
            .filter(|d| (d.total_space() as f64) >= 1.6e10 && (d.total_space() as f64) < 1.0e12)
            .filter(|p| p.is_removable())
            .collect::<Vec<&Disk>>()
    }

    #[func]
    fn drives_list_names() -> PackedStringArray {
        let mut array = PackedStringArray::new();
        for disk in Self::get_disk_list(&Disks::new_with_refreshed_list()) {
            array.push(&godot_str!(
                "{} ({}) [{}]",
                disk.name().to_string_lossy(),
                disk.mount_point().to_string_lossy(),
                human_bytes::human_bytes(disk.total_space() as f64)
            ));
        }
        array
    }

    #[func]
    fn drives_list_paths() -> PackedStringArray {
        let mut array = PackedStringArray::new();
        for disk in Self::get_disk_list(&Disks::new_with_refreshed_list()) {
            array.push(&GString::from(
                &disk.mount_point().to_string_lossy().to_string(),
            ));
        }
        array
    }

    /// Returns the path to [command] if the command exists on the system,
    /// otherwise returns an empty string.
    #[func]
    fn which(command: GString) -> GString {
        match which::which(command.to_string()) {
            Ok(path) => GString::from(&path.to_string_lossy().to_string()),
            Err(_) => GString::new(),
        }
    }
}

#[derive(GodotClass)]
#[class(no_init, base=Object)]
struct M8ToolTaskList {
    params: task::Params,
    tasks: Vec<Box<dyn Task>>,
    task_thread: Option<std::thread::JoinHandle<()>>,
    current_task_index: usize,
    current_task_start_time: std::time::Instant,
    current_task_process: Option<TaskProcess>,
    is_running: bool,
    base: Base<Object>,
}

#[godot_api]
impl M8ToolTaskList {
    #[signal]
    fn received_progress(dict: VarDictionary);

    #[signal]
    fn received_log(message: String);

    #[func]
    fn create(source_path: String, backup_path: String, dry_run: bool) -> Option<Gd<Self>> {
        let params = task::Params::new(source_path.into(), backup_path.into())
            .is_verbose(true)
            .is_dry_run(dry_run)
            .build()
            .ok()?;

        Some(Gd::from_init_fn(|base| Self {
            params,
            tasks: Vec::new(),
            task_thread: None,
            current_task_index: 0,
            current_task_start_time: std::time::Instant::now(),
            current_task_process: None,
            is_running: false,
            base,
        }))
    }

    #[func]
    fn add_backup_task(&mut self) {
        self.add_task(crate::TaskType::Backup.create());
    }

    #[func]
    fn add_clean_task(&mut self) {
        self.add_task(crate::TaskType::Clean.create());
    }

    #[func]
    fn add_optimize_task(&mut self) {
        self.add_task(crate::TaskType::Optimize.create());
    }

    #[func]
    fn add_shrink_task(&mut self) {
        self.add_task(crate::TaskType::Shrink.create());
    }

    #[func]
    fn get_task_names(&self) -> PackedStringArray {
        self.tasks.iter().map(|t| GString::from(t.name())).collect()
    }

    #[func]
    fn process(&mut self) {
        if !self.is_running || self.tasks.is_empty() {
            // godot_print!("tasklist: no tasks to process or already running");
            return;
        }

        if self.current_task_index >= self.tasks.len() {
            godot_print!("tasklist: all tasks completed");
            self.is_running = false;
            return;
        }

        let messages = self.receive_messages();
        let current_task_start_time = self.current_task_start_time.clone();

        if messages.is_empty()
            && let Some(process) = &self.current_task_process
            && !process.is_running()
        {
            let elapsed = current_task_start_time.elapsed();
            let seconds = elapsed.as_secs() % 60;
            let minutes = (elapsed.as_secs() / 60) % 60;
            let hours = elapsed.as_secs() / 3600;
            // process..log(format!(
            //     "task '{}' completed in {:02}:{:02}:{:02}",
            //     current_task.name(),
            //     hours,
            //     minutes,
            //     seconds
            // ));
            self.start_next();
        }

        messages.iter_shared().for_each(|message| {
            if let Some(message_type) = message.get("type") {
                match message_type.to_string().as_str() {
                    "progress" => self.signals().received_progress().emit(&message.clone()),
                    "log" => {
                        if let Some(text) = message.get("text") {
                            self.signals().received_log().emit(text.to_string());
                        }
                    }
                    _ => {}
                }
            }
        });
    }

    #[func]
    fn start(&mut self) {
        self.kill();
        if self.tasks.is_empty() {
            godot_print!("tasklist: no tasks to start");
            return;
        }
        self.start_next();
    }

    #[func]
    fn kill(&mut self) {
        if self.is_running {
            if let Some(process) = &mut self.current_task_process {
                if process.is_running() {
                    godot_print!("tasklist: stopping current task '{}'", process.name());
                    let _ = process.kill();
                    godot_print!("tasklist: stopped all tasks");
                }
            }
        }
        self.is_running = false;
    }

    #[func]

    fn is_running(&self) -> bool {
        self.is_running
    }

    /// Start the next task in the list.
    /// If this task list is not running, starts the first task.
    fn start_next(&mut self) {
        if !self.is_running {
            godot_print!("tasklist: running task(s)...");
            self.current_task_index = 0;
            self.is_running = true;
        } else {
            if self.is_current_task_running() {
                godot_print!(
                    "tasklist: current task '{}' is still running, waiting to finish...",
                    self.current_task().unwrap().name()
                );
                return;
            }
            self.current_task_index += 1;
        }

        if self.current_task_index >= self.tasks.len() {
            godot_print!("tasklist: finished all tasks");
            self.is_running = false;
            return;
        }

        let current_task = &mut self.tasks[self.current_task_index];
        match current_task.spawn(&self.params) {
            Ok(process) => {
                godot_print!("tasklist: starting task: {}", current_task.name());
                self.current_task_start_time = std::time::Instant::now();
                self.current_task_process = Some(process);
            }
            Err(e) => {
                godot_error!("Error starting task {}: {}", current_task.name(), e);
                self.is_running = false;
            }
        }
    }

    fn current_task(&mut self) -> Option<&mut Box<dyn Task>> {
        if self.current_task_index < self.tasks.len() {
            Some(&mut self.tasks[self.current_task_index])
        } else {
            None
        }
    }

    fn is_current_task_running(&mut self) -> bool {
        if let Some(process) = &self.current_task_process {
            process.is_running()
        } else {
            false
        }
    }

    #[func]
    fn receive_messages(&mut self) -> Array<VarDictionary> {
        let Some(process) = &mut self.current_task_process else {
            return array![];
        };
        let mut array = Array::new();
        for message in process.receive_messages() {
            let mut dict = VarDictionary::new();
            match message {
                TaskMessage::Progress {
                    task_name,
                    file,
                    count,
                    total,
                    file_status,
                    metadata,
                } => {
                    let _ = dict.insert("type", "progress");

                    let _ = dict.insert("task_name", task_name);
                    let _ = dict.insert("file", file);
                    let _ = dict.insert("count", count as u32);
                    let _ = dict.insert("total", total as u32);
                    let _ = dict.insert(
                        "percent",
                        if total > 0 {
                            (count as f64 / total as f64) * 100.0
                        } else {
                            0.0
                        },
                    );
                    let _ = dict.insert("metadata", metadata.unwrap_or_default());
                    let _ = dict.insert(
                        "status",
                        match file_status {
                            FileStatus::Unchanged => "good",
                            FileStatus::Changed => "converted",
                            FileStatus::Skipped(_) => "skipped",
                            FileStatus::Removed => "removed",
                            FileStatus::Renamed(_) => "renamed",
                        },
                    );
                    if let FileStatus::Skipped(reason) = file_status {
                        let _ = dict.insert("skip_reason", reason);
                    }
                }
                TaskMessage::Log(info) => {
                    let _ = dict.insert("type", "log");
                    let _ = dict.insert("text", ansi_to_bbcode(&info));
                }
            }
            array.push(&dict);
        }
        array
    }

    fn add_task(&mut self, task: Box<dyn Task>) {
        self.tasks.push(task);
    }
}

#[godot_api(secondary)]
impl M8ToolTaskList {
    #[func]
    fn get_optimize_whitelisted_dirs(&self) -> PackedStringArray {
        self.params
            .optimize_whitelisted_dirs
            .iter()
            .map(|s| GString::from(s))
            .collect()
    }

    #[func]
    fn get_optimize_target_bit_depth(&self) -> u16 {
        self.params.target_bit_depth
    }

    #[func]
    fn get_optimize_target_sample_rate(&self) -> u32 {
        self.params.target_sample_rate
    }

    #[func]
    fn get_optimize_dual_mono_enabled(&self) -> bool {
        self.params.optimize_dual_mono_samples_enabled
    }

    #[func]
    fn get_shrink_whitelisted_dirs(&self) -> PackedStringArray {
        self.params
            .shrink_whitelisted_dirs
            .iter()
            .map(|s| GString::from(s))
            .collect()
    }

    #[func]
    fn get_shrink_remove_common_prefixes(&self) -> bool {
        self.params.remove_common_prefixes
    }

    #[func]
    fn get_shrink_remove_common_suffixes(&self) -> bool {
        self.params.remove_common_suffixes
    }

    #[func]
    fn set_optimize_whitelisted_dirs(&mut self, dirs: PackedStringArray) {
        self.params.optimize_whitelisted_dirs = dirs.to_string_vec()
    }

    #[func]
    fn set_optimize_target_bit_depth(&mut self, bit_depth: u16) {
        self.params.target_bit_depth = bit_depth
    }

    #[func]
    fn set_optimize_target_sample_rate(&mut self, sample_rate: u32) {
        self.params.target_sample_rate = sample_rate
    }

    #[func]
    fn set_optimize_dual_mono_enabled(&mut self, enabled: bool) {
        self.params.optimize_dual_mono_samples_enabled = enabled
    }

    #[func]
    fn set_shrink_whitelisted_dirs(&mut self, dirs: PackedStringArray) {
        self.params.shrink_whitelisted_dirs = dirs.to_string_vec()
    }

    #[func]
    fn set_shrink_remove_common_prefixes(&mut self, enabled: bool) {
        self.params.remove_common_prefixes = enabled
    }

    #[func]
    fn set_shrink_remove_common_suffixes(&mut self, enabled: bool) {
        self.params.remove_common_suffixes = enabled
    }
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

trait PackedStringArrayExt {
    fn to_string_vec(&self) -> Vec<String>;
}

impl PackedStringArrayExt for PackedStringArray {
    fn to_string_vec(&self) -> Vec<String> {
        self.to_vec().iter().map(|s| s.to_string()).collect()
    }
}
