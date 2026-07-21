use clap::{Parser, Subcommand};
use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use m8tool::*;
use std::path::PathBuf;
use std::process;
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

fn main() {
    if let Err(e) = run() {
        eprintln!(
            "{}",
            style(format!("{}: {}", style(" ✘ error").bold(), e)).red()
        );
        process::exit(1);
    }
}

struct Config {
    backup_path: PathBuf,
}

fn run() -> Result<(), Error> {
    let mut cli = Cli::parse();
    let args = &mut cli.args;
    let is_dry_run = args.dry_run;

    let bar_style = ProgressStyle::with_template(" {spinner} {msg}")
        .unwrap()
        .progress_chars("##-");

    let config = Config {
        backup_path: PathBuf::from(&args.backup_path),
    };

    println!();

    // find an M8 drive if none was provided and check if it exists

    let bar = ProgressBar::new_spinner().with_style(bar_style.clone());
    bar.enable_steady_tick(Duration::from_millis(100));

    if args.input_path.is_none() {
        println!(":: finding an SD card...");
        bar.set_message("finding an SD card...");

        let mount_points = m8tool::find_m8_sd_card_mount_points();
        if mount_points.is_empty() {
            return bar.finish_err("could not find an SD card");
        } else if mount_points.len() > 1 {
            return bar.finish_err(format!(
                "found multiple M8 SD cards: {}",
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

    bar.set_message("checking backup path...");
    let input_path = args.input_path.clone().unwrap();
    if !input_path.exists() {
        return bar.finish_err(format!(
            "input path does not exist: {}",
            input_path.to_string_lossy()
        ));
    }

    bar.finish_ok(format!("input path » {}", input_path.to_string_lossy()));

    // check if the backup path exists

    let bar = ProgressBar::new_spinner().with_style(bar_style.clone());
    bar.enable_steady_tick(Duration::from_millis(100));
    bar.set_message("checking backup path...");

    if !config.backup_path.exists() {
        return bar.finish_err(format!(
            "backup path does not exist: {}",
            config.backup_path.to_string_lossy()
        ));
    } else {
        bar.finish_ok(format!(
            "backup path » {}",
            config.backup_path.to_string_lossy()
        ));
    }

    // create tasks

    if is_dry_run {
        println!(
            ":: {}",
            style("note: dry run is enabled. nothing will actually be done").yellow()
        );
    }

    let task_names = if cli.commands.len() == 0 {
        vec!["backup"]
    } else {
        cli.commands.iter().map(|s| s.as_str()).collect()
    };

    let task_params = TaskParams::new(input_path, config.backup_path, args.dry_run, args.verbose)?;

    let mut tasks: Vec<(&str, Box<dyn Task>)> = vec![];
    for task_name in task_names.as_slice() {
        tasks.push(match task_name {
            &"backup" => (
                "backup",
                Box::new(BackupTask::from_params(task_params.clone()).unwrap()),
            ),
            &"optimize" => (
                "optimize",
                Box::new(OptimizeTask::from_params(task_params.clone().into()).unwrap()),
            ),
            &"shrink" => (
                "shrink",
                Box::new(ShrinkTask::from_params(task_params.clone().into()).unwrap()),
            ),
            &"clean" => (
                "clean",
                Box::new(CleanTask::from_params(task_params.clone()).unwrap()),
            ),
            _ => {
                return Err(Error::new(format!("unknown command: {}", task_name)));
            }
        });
    }

    println!(":: starting tasks: {}", &task_names.join(" -> "));

    let total_tasks = tasks.len();
    let mut current_task = 1;

    for (task_name, mut task) in tasks {
        task.start()?;

        let task_progress = format!("[{current_task}/{total_tasks}]");

        let mb = MultiProgress::new();

        let bar = mb.add(ProgressBar::new_spinner().with_style(bar_style.clone()));
        bar.enable_steady_tick(Duration::from_millis(100));

        bar.set_message(format!(
            "{} {}",
            task_progress,
            match task_name {
                "backup" => "backing up files...",
                "optimize" => "optimizing samples...",
                "shrink" => "shrinking sample paths...",
                "clean" => "cleaning up extra files...",
                _ => "unknown task...",
            }
        ));

        let pbar = mb.add(ProgressBar::new(task.paths().len() as u64));
        pbar.reset();
        pbar.set_style(
            ProgressStyle::with_template("   {wide_msg} {elapsed_precise} [{bar:40}] {percent}%")
                .unwrap()
                .progress_chars("##-"),
        );

        loop {
            let messages = task.receive_messages();
            for msg in &messages {
                match msg {
                    TaskMessage::Log(message) => {
                        if task.is_verbose() {
                            mb.println(message)?;
                        }
                    }
                    TaskMessage::Progress { file, .. } => {
                        pbar.set_message(shrink_path(&file));
                        pbar.inc(1);
                    }
                }
            }
            if !task.is_running() && messages.is_empty() {
                break;
            }
        }

        bar.disable_steady_tick();
        pbar.finish_and_clear();

        let result = task.join()?;
        let task_backup_path = task
            .task_backup_path()
            .expect("task backup path should be set after task is complete");

        {
            let t = task_progress;
            let n = style(result.paths_modified.len()).bold();
            let p = style(task_backup_path.to_string()).bold();

            let message = match task_name {
                "backup" => format!("{t} {n} files backed up to {p}"),
                "optimize" => format!("{t} {n} optimized. originals backed up to {p}"),
                "shrink" => format!("{t} {n} paths changed. removed files backed up to {p}"),
                "clean" => format!("{t} {n} files cleaned. removed files backed up to {p}"),
                _ => format!("{t} {n} {p}"),
            };

            bar.finish_ok(message);
        }

        current_task += 1;
    }

    Ok(())
}

trait ProgressBarExt {
    fn finish_ok(&self, msg: impl Into<String>);
    fn finish_err(&self, msg: impl Into<String>) -> Result<(), Error>;
}

impl ProgressBarExt for ProgressBar {
    fn finish_ok(&self, msg: impl Into<String>) {
        let bar_style_ok =
            ProgressStyle::with_template(&format!(" {} {}", style("✔").green(), "{msg}")).unwrap();
        self.set_style(bar_style_ok);
        self.finish_with_message(msg.into());
    }

    fn finish_err(&self, msg: impl Into<String>) -> Result<(), Error> {
        let msg = &msg.into();
        // let bar_style_err = ProgressStyle::with_template(" ✘ {msg}").unwrap();
        // self.set_style(bar_style_err);
        // self.finish_with_message(msg.clone());
        self.finish_and_clear();
        Err(Error::new(msg))
    }
}

fn shrink_path(path: &String) -> String {
    use shrinkpath::{ShrinkOptions, Strategy, shrink};
    let shrink_opts = ShrinkOptions::new(80)
        .strategy(Strategy::Ellipsis)
        .anchor("Samples")
        .anchor("Instruments")
        .anchor("Renders")
        .anchor("Bundles")
        .anchor("Songs");

    shrink(&path, &shrink_opts)
}
