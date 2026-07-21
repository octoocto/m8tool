use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    vec::IntoIter,
};

use crate::task;
use anyhow::Result;

pub struct TaskList {
    tasks: IntoIter<Box<dyn task::Task>>,
    params: task::Params,
    should_run: Arc<AtomicBool>,
    handler: Box<dyn TaskListHandler>,
}

impl TaskList {
    pub fn new(
        tasks: Vec<Box<dyn task::Task>>,
        params: task::Params,
        handler: impl TaskListHandler + 'static,
    ) -> Self {
        let tasks = tasks.into_iter();
        Self {
            params,
            tasks,
            should_run: Arc::new(AtomicBool::new(true)),
            handler: Box::new(handler),
        }
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn should_run_cloned(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.should_run)
    }

    /// Runs all the tasks sequentually and waits for each one to finish.
    pub fn run_tasks(&mut self) -> Result<()> {
        let mut current_task = 0;
        let total_tasks = self.tasks.len();
        while let Some(task) = &self.tasks.next() {
            let mut process = task.spawn(&self.params)?;

            self.handler
                .on_task_start(task, process.paths(), current_task, total_tasks);

            'reader: loop {
                let messages = process.receive_messages();
                if !process.is_running() && messages.is_empty() {
                    break;
                }
                for message in &messages {
                    if !self.should_run() {
                        process.kill();
                        break 'reader;
                    }
                    self.handler.on_receive_message(&self.params, message)?;
                }
            }

            let result = process.join()?;
            self.handler
                .on_task_finish(task, &result, current_task, total_tasks);

            current_task += 1;
        }
        Ok(())
    }

    // pub fn run_next_task(&mut self) -> Result<task::TaskResult> {
    //     match &self.tasks.next() {
    //         Some(task) => {
    //             let mut process = task.spawn(&self.params)?;
    //             'reader: loop {
    //                 let messages = process.receive_messages();
    //                 if !process.is_running() && messages.is_empty() {
    //                     break;
    //                 }
    //                 for message in &messages {
    //                     if !self.should_run() {
    //                         process.kill();
    //                         break 'reader;
    //                     }
    //                     (&mut self.on_receive_message)(&self.params, message);
    //                 }
    //             }
    //
    //             Ok(process.join()?)
    //         }
    //         None => bail!("No tasks to run"),
    //     }
    // }
    //
    fn should_run(&self) -> bool {
        self.should_run.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn interrupt(&mut self) {
        self.should_run
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

pub trait TaskListHandler {
    fn on_receive_message(
        &mut self,
        params: &task::Params,
        message: &task::TaskMessage,
    ) -> Result<()>;
    fn on_task_start(
        &mut self,
        task: &Box<dyn task::Task>,
        paths: &Vec<PathBuf>,
        current_task: usize,
        total_tasks: usize,
    );
    fn on_task_finish(
        &mut self,
        task: &Box<dyn task::Task>,
        result: &task::TaskResult,
        current_task: usize,
        total_tasks: usize,
    );
}
