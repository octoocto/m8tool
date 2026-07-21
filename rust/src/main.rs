use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use m8tool::*;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::OnceLock;
use std::time::Duration;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    commands: Vec<String>,

    #[command(flatten)]
    args: CliArgs,
}

#[derive(Parser)]
struct CliArgs {
    #[arg(short, help = "Path to the source directory")]
    input_path: Option<PathBuf>,

    #[arg(short, help = "Path to the backup directory")]
    backup_path: PathBuf,

    #[arg(
        short,
        long,
        default_value_t = false,
        help = "Perform a dry run without making any changes"
    )]
    dry_run: bool,

    #[arg(
        short,
        long,
        default_value_t = false,
        help = "Enable verbose output for debugging purposes"
    )]
    verbose: bool,
}

#[derive(Subcommand)]
enum Command {
    Backup,
    Clean,
    Optimize,
    Shrink,
}

struct Config {
    backup_path: PathBuf,
}

fn main() {
    if let Err(e) = run() {
        println!();
        println!();
        eprintln!(
            "{}",
            style(format!("{}: {}", style("error").bold(), e)).red()
        );
        process::exit(1);
    }
}

fn style_spinner() -> &'static ProgressStyle {
    static STYLE: OnceLock<ProgressStyle> = OnceLock::new();
    STYLE.get_or_init(|| {
        ProgressStyle::with_template(" {spinner} {msg}")
            .unwrap()
            .progress_chars("##-")
    })
}

fn style_progress() -> &'static ProgressStyle {
    static STYLE: OnceLock<ProgressStyle> = OnceLock::new();
    STYLE.get_or_init(|| {
        ProgressStyle::with_template(
            " {spinner} {wide_msg} {elapsed_precise} [{bar:40}] {percent}%",
        )
        .unwrap()
        .progress_chars("##-")
    })
}

fn style_raw() -> &'static ProgressStyle {
    static STYLE: OnceLock<ProgressStyle> = OnceLock::new();
    STYLE.get_or_init(|| {
        ProgressStyle::with_template("{msg}")
            .unwrap()
            .progress_chars("##-")
    })
}

fn run() -> Result<()> {
    let mut cli = Cli::parse();
    let args = &mut cli.args;
    let is_dry_run = args.dry_run;

    let config = Config {
        backup_path: PathBuf::from(&args.backup_path),
    };

    println!();

    // find an M8 drive if none was provided and check if it exists

    let bar = ProgressBar::new_spinner().with_style(style_spinner().clone());
    bar.enable_steady_tick(Duration::from_millis(100));

    if args.input_path.is_some() {
        bar.set_message("checking input path...");
    } else {
        bar.set_message("finding an SD card...");

        let mount_points = m8tool::find_m8_sd_card_mount_points();
        if mount_points.is_empty() {
            return bar.finish_err("none found.");
        } else if mount_points.len() > 1 {
            return bar.finish_err(format!(
                "found multiple: {}",
                mount_points
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            ));
        }
        args.input_path = mount_points.first().cloned();
        assert!(args.input_path.is_some());
    }

    let input_path = args.input_path.clone().unwrap();
    if !input_path.exists() {
        return bar.finish_err(format!(
            "path does not exist: {}",
            input_path.to_string_lossy()
        ));
    }

    bar.finish_ok(input_path.to_string_lossy());

    // check if the backup path exists

    let bar = ProgressBar::new_spinner().with_style(style_spinner().clone());
    bar.enable_steady_tick(Duration::from_millis(100));
    bar.set_message("checking backup path...");

    if !config.backup_path.exists() {
        return bar.finish_err(format!(
            "path does not exist: {}",
            config.backup_path.to_string_lossy()
        ));
    } else {
        bar.finish_ok(config.backup_path.to_string_lossy());
    }
    println!();
    println!();

    // create tasks

    if is_dry_run {
        println!(
            ":: {}",
            style("note: dry run is enabled. nothing will actually be done").yellow()
        );
    }

    let task_types: Vec<TaskType> = if cli.commands.len() == 0 {
        vec!["backup"]
    } else {
        cli.commands.iter().map(|s| s.as_str()).collect()
    }
    .iter()
    .map(|n| TaskType::from_str(n).ok_or(anyhow::anyhow!("unknown command: {n}")))
    .collect::<Result<Vec<TaskType>>>()?;

    let task_params = Params::new(input_path.clone(), config.backup_path.clone())
        .is_dry_run(args.dry_run)
        .is_verbose(args.verbose)
        .build()?;

    let tasks: Vec<Box<dyn Task>> = task_types.iter().map(|t| t.create()).collect();
    let mut task_list = TaskList::new(tasks, task_params, CliTaskListHandler::new());

    // set up ctrl-c handler
    let should_run = task_list.should_run_cloned();
    ctrlc::set_handler(move || {
        should_run.store(false, std::sync::atomic::Ordering::Relaxed);
    })?;

    task_list.run_tasks()?;

    println!();
    Ok(())
}

struct CliTaskListHandler {
    mp: MultiProgress,
    bar0: ProgressBar,
    bar1: ProgressBar,
}

impl CliTaskListHandler {
    fn new() -> Self {
        let mp = MultiProgress::new();
        Self {
            bar0: mp.add(ProgressBar::new(0).with_style(style_progress().clone())),
            bar1: mp.add(ProgressBar::new_spinner().with_style(style_raw().clone())),
            mp,
        }
    }
}

impl TaskListHandler for CliTaskListHandler {
    fn on_task_start(
        &mut self,
        task: &Box<dyn Task>,
        paths: &Vec<PathBuf>,
        current: usize,
        total: usize,
    ) {
        let task_progress = format!("[{current}/{total}]");
        self.bar0.set_length(paths.len() as u64);
        self.bar0
            .set_message(format!("{} {}", task_progress, task.start_message()));
        self.bar1.set_message("filename");
    }

    fn on_task_finish(
        &mut self,
        task: &Box<dyn Task>,
        result: &TaskResult,
        current: usize,
        total: usize,
    ) {
        let task_progress = format!("[{current}/{total}]");
        let t = task_progress;
        let e = {
            let elapsed_secs = self.bar0.elapsed().as_secs();
            let hours = elapsed_secs / 3600;
            let minutes = (elapsed_secs % 3600) / 60;
            let seconds = elapsed_secs % 60;
            format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
        };

        self.bar0.clear_message();

        self.bar0.finish_ok(if result.interrupted {
            format!("{t} {} has been interrupted", style(task.name()).bold(),)
        } else {
            format!(
                "{t} {} completed in {}",
                style(task.name()).bold(),
                style(e).bold()
            )
        });

        println!("   {t} {}", task.finish_message(&result));
    }

    fn on_receive_message(&mut self, params: &Params, message: &TaskMessage) -> Result<()> {
        match message {
            TaskMessage::Log(message) => {
                if params.is_verbose {
                    self.mp.println(message)?;
                }
            }
            TaskMessage::Progress {
                file, file_status, ..
            } => {
                let mp = &self.mp;
                self.bar1.set_message(shrink_path(Path::new(&file))?);
                self.bar0.inc(1);
                if params.is_verbose {
                    match file_status {
                        FileStatus::Unchanged => {}
                        FileStatus::Skipped(reason) => {
                            mp.println(format!("{} {}", style("skipped:").white().bold(), file))?;
                            mp.println(format!("{} {}", style("       └").white().bold(), reason))?;
                        }
                        FileStatus::Changed => {
                            mp.println(format!("{} {}", style("changed:").green().bold(), file))?;
                        }
                        FileStatus::Removed => {
                            mp.println(format!("{} {}", style("removed:").yellow().bold(), file))?;
                        }
                        FileStatus::Renamed(new_path) => {
                            mp.println(format!("{} {file}", style("renamed:").yellow().bold(),))?;
                            mp.println(format!(
                                "{} {}",
                                style("   └ to:").yellow().bold(),
                                style(new_path).yellow(),
                            ))?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

trait ProgressBarExt {
    fn clear_message(&self);
    // fn append_message(&self, msg: impl Into<String>);
    fn finish_ok(&self, msg: impl Into<String>);
    fn finish_err(&self, msg: impl Into<String>) -> Result<()>;
}

impl ProgressBarExt for ProgressBar {
    fn clear_message(&self) {
        self.set_message("");
    }

    fn finish_ok(&self, msg: impl Into<String>) {
        let bar_style_ok =
            ProgressStyle::with_template(&format!(" {} {}", style("✔").green(), "{msg}")).unwrap();
        let new_msg = format!("{}{}", self.message(), msg.into());
        self.set_style(bar_style_ok);
        self.finish_with_message(new_msg);
    }

    fn finish_err(&self, msg: impl Into<String>) -> Result<()> {
        let bar_style_err =
            ProgressStyle::with_template(&format!(" {} {}", style("✘").red(), "{msg}")).unwrap();
        let new_msg = format!("{}{}", self.message(), msg.into());
        self.set_style(bar_style_err);
        self.finish_with_message(new_msg.clone());
        bail!(new_msg)
    }
}

fn shrink_path(path: &Path) -> Result<String> {
    use shrinkpath::{ShrinkOptions, Strategy, shrink};
    use std::path::Component;

    let length = 60;
    let mut shrink_opts = ShrinkOptions::new(length).strategy(Strategy::Ellipsis);

    for dirname in ["samples", "instruments", "renders", "bundles", "songs"] {
        if let Some(first_dir) = path
            .components()
            .find(|c| matches!(c, Component::Normal(_)))
            && first_dir.as_os_str().to_string_lossy().to_lowercase() == dirname
        {
            shrink_opts.max_len -= first_dir.as_os_str().len();
            return Ok(format!(
                "{}/{}",
                first_dir.as_os_str().to_string_lossy(),
                shrink(
                    &path.strip_prefix(first_dir)?.to_owned().to_string(),
                    &shrink_opts,
                )
            ));
        }
    }

    Ok(shrink(&path.to_owned().to_string(), &shrink_opts))
}
