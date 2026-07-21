mod backup;
mod clean;
mod optimize;
mod params;
mod shrink;
mod task;
mod task_list;

pub use params::Params;
pub use params::ParamsBuilder;
pub use task_list::{TaskList, TaskListHandler};

pub use task::*;

pub enum TaskType {
    Backup,
    Clean,
    Optimize,
    Shrink,
}

impl TaskType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "backup" => Some(TaskType::Backup),
            "clean" => Some(TaskType::Clean),
            "optimize" => Some(TaskType::Optimize),
            "shrink" => Some(TaskType::Shrink),
            _ => None,
        }
    }

    pub fn create(&self) -> Box<dyn Task> {
        match self {
            TaskType::Backup => Box::new(backup::BackupTask),
            TaskType::Clean => Box::new(clean::CleanTask),
            TaskType::Optimize => Box::new(optimize::OptimizeTask),
            TaskType::Shrink => Box::new(shrink::ShrinkTask),
        }
    }
}
