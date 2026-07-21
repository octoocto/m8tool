use crate::{FileStatus, Params, PathBufExt, generate_backup_name};
use anyhow::{Result, bail};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool, mpsc},
    time::Duration,
};

pub type TaskThread = std::thread::JoinHandle<Result<TaskResult>>;
pub type TaskFn = Box<dyn FnOnce(TaskProcessHandle) -> Result<TaskResult> + Send + 'static>;

pub type MessageReceiver = std::sync::mpsc::Receiver<TaskMessage>;
pub type MessageSender = std::sync::mpsc::Sender<TaskMessage>;

pub trait Task {
    fn name(&self) -> &str;

    fn start_message(&self) -> &str;

    fn finish_message(&self, result: &TaskResult) -> String;

    /// Get the paths to files and/or directories that will be processed by
    /// this task.
    ///
    /// Paths in this list could be deleted or modified by the task.
    fn collect_paths(&self, params: &Params) -> Vec<PathBuf>;

    /// Start the task. This will usually be non-blocking and spawn a child process.
    fn spawn(&self, params: &Params) -> Result<TaskProcess>;

    fn thread_fn(&self, params: &Params) -> TaskFn;

    /// Generate a backup path for this task.
    ///
    fn generate_backup_path(&self, params: &Params) -> PathBuf {
        loop {
            let task_backup_path = params
                .backup_path
                .join(generate_backup_name(self.name()))
                .with_trailing_separator();
            if !task_backup_path.exists() {
                return task_backup_path;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[derive(Clone)]
pub struct TaskProcessHandle {
    verbose: bool,
    paths: Vec<PathBuf>,
    task_backup_path: PathBuf,
    should_run: Arc<AtomicBool>,
    sender: MessageSender,
}

impl TaskProcessHandle {
    fn new(task: &impl Task, params: &Params, sender: MessageSender) -> Self {
        Self {
            paths: task.collect_paths(params).clone(),
            verbose: params.is_verbose,
            task_backup_path: task.generate_backup_path(params),
            should_run: Arc::new(AtomicBool::new(true)),
            sender,
        }
    }

    pub fn should_run(&self) -> bool {
        self.should_run.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn request_stop(&mut self) {
        self.should_run
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn task_backup_path(&self) -> &PathBuf {
        &self.task_backup_path
    }

    pub fn paths(&self) -> &Vec<PathBuf> {
        &self.paths
    }
}

impl TaskProcessHandle {
    pub fn finished(
        self,
        paths_processed: Vec<PathBuf>,
        paths_modified: Vec<PathBuf>,
    ) -> TaskResult {
        TaskResult {
            task_backup_path: self.task_backup_path,
            paths_processed,
            paths_modified,
            interrupted: false,
        }
    }

    pub fn interrupted(
        self,
        paths_processed: Vec<PathBuf>,
        paths_modified: Vec<PathBuf>,
    ) -> TaskResult {
        TaskResult {
            task_backup_path: self.task_backup_path,
            paths_processed,
            paths_modified,
            interrupted: true,
        }
    }
}

impl TaskProcessHandle {
    pub fn log<T>(&self, message: T)
    where
        T: ToString,
    {
        if let Err(e) = self.sender.send(TaskMessage::Log(message.to_string())) {
            eprintln!("Error sending message: {}", e);
        }
    }

    pub fn loga<T>(&mut self, messages: &[T])
    where
        T: ToString,
    {
        for message in messages {
            self.log(message.to_string());
        }
    }

    pub fn logv<T>(&mut self, message: T)
    where
        T: ToString,
    {
        if self.verbose {
            self.log(message);
        }
    }

    pub fn logva<T>(&mut self, messages: &[T])
    where
        T: ToString,
    {
        if self.verbose {
            self.loga(messages);
        }
    }

    pub fn send_progress(&self, task_name: &str, file: String, count: usize, total: usize) {
        if let Err(e) = self.sender.send(TaskMessage::Progress {
            task_name: task_name.to_string(),
            file,
            count,
            total,
            file_status: FileStatus::Unchanged,
            metadata: None,
        }) {
            eprintln!("Error sending progress message: {}", e);
        }
    }

    pub fn send_progress_with_meta(
        &self,
        task_name: &str,
        file: String,
        count: usize,
        total: usize,
        file_status: FileStatus,
        metadata: String,
    ) {
        if let Err(e) = self.sender.send(TaskMessage::Progress {
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

pub struct TaskProcess {
    name: String,
    thread: Option<TaskThread>,
    channel: TaskMessageChannel,
    handle: TaskProcessHandle,
}

impl TaskProcess {
    pub fn new(task: &impl Task, params: &Params) -> Self {
        let func = task.thread_fn(params);
        let channel = TaskMessageChannel::new();
        let handle = TaskProcessHandle::new(task, params, channel.sender().clone());
        let handle_clone = handle.clone();

        Self {
            name: task.name().to_string(),
            thread: Some(std::thread::spawn(move || func(handle_clone))),
            handle,
            channel,
        }
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn paths(&self) -> &Vec<PathBuf> {
        &self.handle.paths
    }

    pub fn task_backup_path(&self) -> &PathBuf {
        &self.handle.task_backup_path
    }

    pub fn is_running(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }

    pub fn kill(&mut self) {
        println!("Killing task process...");
        self.handle.request_stop();
    }

    pub fn join(&mut self) -> Result<TaskResult> {
        if let Some(thread) = self.thread.take() {
            let result = thread.join();
            match result {
                Ok(Ok(task_result)) => {
                    return Ok(task_result);
                }
                Ok(Err(e)) => {
                    return Err(e);
                }
                Err(e) => {
                    bail!(
                        "Failed to join task thread: {:?}",
                        e.downcast_ref::<String>()
                    );
                }
            }
        } else {
            bail!("Task thread already joined");
        }
    }

    pub fn receive_messages(&mut self) -> Vec<TaskMessage> {
        self.channel.receive_messages()
    }
}

pub struct TaskMessageChannel {
    pub sender: MessageSender,
    pub receiver: MessageReceiver,
}

impl TaskMessageChannel {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self { sender, receiver }
    }

    fn sender(&self) -> &MessageSender {
        &self.sender
    }

    /// Receive all pending messages from this process.
    pub fn receive_messages(&mut self) -> Vec<TaskMessage> {
        let mut messages = Vec::new();
        loop {
            match self.receiver.try_recv() {
                Ok(message) => messages.push(message),
                Err(mpsc::TryRecvError::Empty) => {
                    // println!("receive_messages = empty");
                    break;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    // println!("receive_messages = disconnected");
                    break;
                }
            }
        }
        messages
    }
}

pub struct TaskResult {
    /// The path to the folder this task used to back up files.
    pub task_backup_path: PathBuf,

    /// The list of paths that were processed by this task.
    pub paths_processed: Vec<PathBuf>,

    /// The list of paths that were modified by this task.
    pub paths_modified: Vec<PathBuf>,

    /// If true, the task was interrupted and exited early.
    pub interrupted: bool,
}

impl TaskResult {
    pub fn finished(
        task_backup_path: PathBuf,
        paths_processed: Vec<PathBuf>,
        paths_modified: Vec<PathBuf>,
    ) -> Self {
        Self {
            task_backup_path,
            paths_processed,
            paths_modified,
            interrupted: false,
        }
    }

    pub fn interrupted(
        task_backup_path: PathBuf,
        paths_processed: Vec<PathBuf>,
        paths_modified: Vec<PathBuf>,
    ) -> Self {
        Self {
            task_backup_path,
            paths_processed,
            paths_modified,
            interrupted: true,
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
