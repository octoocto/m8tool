use crate::{
    BackupTask, CleanTask, Error, FileStatus, OptimizeTask, ShrinkTask, Task, TaskMessage,
};
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
    source_path: String,
    backup_path: String,
    dry_run: bool,
    tasks: Vec<Box<dyn Task>>,
    current_task_index: usize,
    current_task_start_time: std::time::Instant,
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
    fn create(source_path: String, backup_path: String, dry_run: bool) -> Gd<Self> {
        Gd::from_init_fn(|base| Self {
            source_path,
            backup_path,
            dry_run,
            tasks: Vec::new(),
            current_task_index: 0,
            current_task_start_time: std::time::Instant::now(),
            is_running: false,
            base,
        })
    }

    #[func]
    fn add_backup_task(&mut self) -> Result<(), GError> {
        self.add_task(BackupTask::new(
            self.source_path.clone().into(),
            self.backup_path.clone().into(),
            self.dry_run,
            true,
        ))
    }

    #[func]
    fn add_clean_task(&mut self) -> Result<(), GError> {
        self.add_task(CleanTask::new(
            self.source_path.clone().into(),
            self.backup_path.clone().into(),
            self.dry_run,
            true,
        ))
    }

    #[func]
    fn add_convert_task(
        &mut self,
        whitelisted_dirs: PackedStringArray,
        target_bit_depth: i32,
        target_sample_rate: i32,
        convert_from_dual_mono: bool,
        convert_other_formats: bool,
    ) -> Result<(), GError> {
        self.add_task(OptimizeTask::new(
            self.source_path.clone().into(),
            self.backup_path.clone().into(),
            self.dry_run,
            true,
            whitelisted_dirs.to_string_vec(),
            target_bit_depth as u16,
            target_sample_rate as u32,
            convert_from_dual_mono,
            convert_other_formats,
        ))
    }

    #[func]
    fn add_shrink_task(
        &mut self,
        whitelisted_dirs: PackedStringArray,
        remove_common_prefixes: bool,
    ) -> Result<(), GError> {
        self.add_task(ShrinkTask::new(
            self.source_path.clone().into(),
            self.backup_path.clone().into(),
            self.dry_run,
            true,
            whitelisted_dirs.to_string_vec(),
            remove_common_prefixes,
        ))
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
            && let Some(current_task) = self.current_task()
            && !current_task.is_running()
        {
            let elapsed = current_task_start_time.elapsed();
            let seconds = elapsed.as_secs() % 60;
            let minutes = (elapsed.as_secs() / 60) % 60;
            let hours = elapsed.as_secs() / 3600;
            current_task.log(format!(
                "task '{}' completed in {:02}:{:02}:{:02}",
                current_task.name(),
                hours,
                minutes,
                seconds
            ));
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
            if let Some(current_task) = self.current_task() {
                if current_task.is_running() {
                    godot_print!("tasklist: stopping current task '{}'", current_task.name());
                    let _ = current_task.kill();
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
        if let Err(e) = current_task.start() {
            godot_error!("Error starting task {}: {}", current_task.name(), e);
            self.is_running = false;
        } else {
            godot_print!("tasklist: starting task: {}", current_task.name());
            self.current_task_start_time = std::time::Instant::now();
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
        if let Some(current_task) = self.current_task() {
            current_task.is_running()
        } else {
            false
        }
    }

    #[func]
    fn receive_messages(&mut self) -> Array<VarDictionary> {
        let Some(current_task) = self.current_task() else {
            return array![];
        };
        let mut array = Array::new();
        for message in current_task.receive_messages() {
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
                            FileStatus::None => "",
                            FileStatus::Good => "good",
                            FileStatus::Converted => "converted",
                            FileStatus::Skipped(_) => "skipped",
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

    fn add_task<T>(&mut self, result: Result<T, Error>) -> Result<(), GError>
    where
        T: Task + 'static,
    {
        result
            .map(|task| self.tasks.push(Box::new(task)))
            .map_err(|_| GError::ERR_INVALID_DATA)
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
