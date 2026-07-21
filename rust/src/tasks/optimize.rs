use crate::Error;
use crate::FileStatus;
use crate::PathBufExt;
use crate::command_run;
use crate::is_handle_running;
use crate::kill_handle;
use crate::tasks::*;
use crate::which;
use console::style;
use fancy_regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::thread;

const TARGET_BIT_DEPTH: u16 = 16;
const TARGET_SAMPLE_RATE: u32 = 44100;

// if enabled, will convert other formats to wav
const OTHER_FORMATS: [&str; 5] = ["mp3", "ogg", "flac", "aiff", "aif"];

// if enabled, will try to detect if stereo files are actually mono and convert them
const DETECT_MONO_THRESHOLD: f32 = -90.0;

const WHITELISTED_DIRS: [&str; 2] = ["samples", "packs"];

const TASK_NAME: &str = "optimize";

enum AudioBitDepth {
    Bit8,
    Bit16,
    Bit24,
    Bit32,
}

impl AudioBitDepth {
    fn from_u16(value: u16) -> Option<Self> {
        match value {
            8 => Some(Self::Bit8),
            16 => Some(Self::Bit16),
            24 => Some(Self::Bit24),
            32 => Some(Self::Bit32),
            _ => None,
        }
    }

    fn to_u16(&self) -> u16 {
        match self {
            Self::Bit8 => 8,
            Self::Bit16 => 16,
            Self::Bit24 => 24,
            Self::Bit32 => 32,
        }
    }
}

enum AudioSampleRate {
    Hz44100,
    Hz48000,
}

impl AudioSampleRate {
    fn from_u32(value: u32) -> Option<Self> {
        match value {
            44100 => Some(Self::Hz44100),
            48000 => Some(Self::Hz48000),
            _ => None,
        }
    }

    fn to_u32(&self) -> u32 {
        match self {
            Self::Hz44100 => 44100,
            Self::Hz48000 => 48000,
        }
    }
}

#[derive(Clone)]
pub struct OptimizeTaskParams {
    source_dir: PathBuf,
    backup_dir: PathBuf,
    is_dry_run: bool,
    is_verbose: bool,
    pub whitelisted_dirs: Vec<String>,
    pub target_bit_depth: u16,
    pub target_sample_rate: u32,
    /// If enabled, will convert other formats to the target format.
    pub convert_from_other_formats_enabled: bool,
    /// If enabled, will try to detect if stereo files are actually mono and convert them.
    pub convert_from_dual_mono_enabled: bool,
}

impl AsTaskParams for OptimizeTaskParams {
    fn source_path(&self) -> PathBuf {
        self.source_dir.clone()
    }

    fn backup_path(&self) -> PathBuf {
        self.backup_dir.clone()
    }

    fn is_dry_run(&self) -> bool {
        self.is_dry_run
    }

    fn is_verbose(&self) -> bool {
        self.is_verbose
    }
}

impl From<TaskParams> for OptimizeTaskParams {
    fn from(params: TaskParams) -> Self {
        Self {
            source_dir: params.source_path(),
            backup_dir: params.backup_path(),
            is_dry_run: params.is_dry_run(),
            is_verbose: params.is_verbose(),
            whitelisted_dirs: WHITELISTED_DIRS.iter().map(|s| s.to_string()).collect(),
            target_bit_depth: TARGET_BIT_DEPTH,
            target_sample_rate: TARGET_SAMPLE_RATE,
            convert_from_other_formats_enabled: true,
            convert_from_dual_mono_enabled: true,
        }
    }
}

pub struct OptimizeTask {
    params: OptimizeTaskParams,
    task_backup_path: Option<PathBuf>,
    paths: Vec<PathBuf>,
    message_channel: (
        std::sync::mpsc::Sender<TaskMessage>,
        std::sync::mpsc::Receiver<TaskMessage>,
    ),
    handle: Option<TaskThread>,
    should_run: Arc<AtomicBool>,
}

impl OptimizeTask {
    pub fn new(
        source_dir: PathBuf,
        backup_dir: PathBuf,
        is_dry_run: bool,
        is_verbose: bool,
        whitelisted_dirs: Vec<String>,
        target_bit_depth: u16,
        target_sample_rate: u32,
        convert_from_other_formats_enabled: bool,
        convert_from_dual_mono_enabled: bool,
    ) -> Result<Self, Error> {
        let params = OptimizeTaskParams {
            source_dir,
            backup_dir,
            is_dry_run,
            is_verbose,
            whitelisted_dirs,
            target_bit_depth,
            target_sample_rate,
            convert_from_other_formats_enabled,
            convert_from_dual_mono_enabled,
        };
        Self::from_params(params)
    }

    pub fn from_params(params: OptimizeTaskParams) -> Result<Self, Error> {
        params.expect_params_are_valid()?;
        match params.target_bit_depth {
            8 | 16 | 24 | 32 => {}
            _ => return Err("target bit depth must be one of: 8, 16, 24, 32".into()),
        }
        match params.target_sample_rate {
            44100 | 48000 => {}
            _ => return Err("target sample rate must be one of: 44100, 48000".into()),
        }
        Ok(Self {
            params,
            task_backup_path: None,
            paths: Vec::new(),
            message_channel: mpsc::channel(),
            handle: None,
            should_run: Arc::new(AtomicBool::new(false)),
        })
    }
}

impl Task for OptimizeTask {
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
        let is_verbose = self.params.is_verbose();

        let message_tx = self.message_tx().clone();

        let task_backup_path = self.generate_backup_path()?;

        source_path.expect_dir_not_empty()?;
        backup_path.expect_dir()?;

        if is_verbose {
            message_tx.log(style(format!("starting task \"{}\"...", self.name())).green());
        }

        // let mut num_processed = 0;
        let mut num_skipped = 0;
        let mut num_good = 0;

        self.paths = collect_audio_files(&source_path);
        let total_files = self.paths.len();

        if is_verbose {
            message_tx.log(format!("ffmpeg: {}", which("ffmpeg")?.display()).to_string());
            message_tx.log(format!("ffprobe: {}", which("ffprobe")?.display()).to_string());
            message_tx.log(
                format!(
                    "whitelisted dirs: {}",
                    self.params.whitelisted_dirs.join(", ")
                )
                .to_string(),
            );
            message_tx
                .log(format!("target bit depth: {} bit", self.params.target_bit_depth).to_string());
            message_tx.log(
                format!("target sample rate: {} Hz", self.params.target_sample_rate).to_string(),
            );
            message_tx.log(
                format!(
                    "convert from other formats enabled: {}",
                    self.params.convert_from_other_formats_enabled
                )
                .to_string(),
            );
            message_tx.log(
                format!(
                    "convert from dual mono enabled: {}",
                    self.params.convert_from_dual_mono_enabled
                )
                .to_string(),
            );
            message_tx.log(format!("total files to process: {}", total_files).to_string());
        }

        let params = self.params.clone();
        let should_run = Arc::new(AtomicBool::new(true));

        self.should_run = should_run.clone();

        self.handle = Some(thread::spawn(move || {
            let mut processed_paths = vec![];
            let mut converted_paths = vec![];
            // optimize wavs
            for path in crate::collect_paths(&source_path) {
                if !should_run.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                if is_wav(&path) {
                    let (status, audio_format) =
                        match Self::convert_file(&message_tx, &params, &path, &task_backup_path) {
                            Ok((was_converted, audio_format)) => {
                                if was_converted {
                                    converted_paths.push(path.clone());
                                    (FileStatus::Converted, Some(audio_format))
                                } else {
                                    num_good += 1;
                                    (FileStatus::Good, Some(audio_format))
                                }
                            }
                            Err(e) => {
                                num_skipped += 1;
                                (FileStatus::Skipped(e.to_string()), None)
                            }
                        };

                    processed_paths.push(path.clone());

                    let audio_format_meta = match audio_format {
                        Some(format) => format!(
                            "{} bit  {} Hz  {}",
                            format.bit_depth,
                            format.sample_rate,
                            match format.channels {
                                1 => "mono".to_string(),
                                2 => "ster".to_string(),
                                n => format!("{:02}ch", n),
                            }
                        ),
                        None => "".to_string(),
                    };

                    let path_relative = path.strip_prefix(&source_path).unwrap_or(&path);

                    message_tx.send_progress_with_meta(
                        TASK_NAME,
                        format!("{}", path_relative.display()),
                        processed_paths.len(),
                        total_files,
                        status,
                        audio_format_meta,
                    );
                } else if let Some(ext) = file_get_extension(&path) {
                    // convert other samples to wav
                    if OTHER_FORMATS.contains(&ext.as_str()) {
                        message_tx.log(style(format!("found: {}", path.display())).yellow());
                    }
                }
            }

            message_tx.log(
                style(format!(
                    "processed {} files ({} converted, {} good, {} skipped)",
                    processed_paths.len(),
                    converted_paths.len(),
                    num_good,
                    num_skipped,
                ))
                .green(),
            );
            message_tx.log(
                style(format!(
                    "backup of original files has been made in: {}",
                    task_backup_path.display()
                ))
                .green(),
            );

            Ok(TaskResult::new(processed_paths, converted_paths))
        }));
        Ok(())
    }

    fn is_running(&mut self) -> bool {
        is_handle_running(&mut self.handle).unwrap_or(false)
    }

    fn kill(&mut self) -> Result<(), Error> {
        kill_handle(&mut self.handle, &mut self.should_run)
    }

    fn join(&mut self) -> Result<TaskResult, Error> {
        if let Some(handle) = self.handle.take() {
            handle
                .join()
                .map_err(|e| Error::new(format!("error joining thread: {:?}", e)))
                .flatten()
        } else {
            Err(Error::new("thread not started"))
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

impl OptimizeTask {
    /// Checks if the path is in a whitelisted directory.
    fn is_valid_path(
        convert_params: &OptimizeTaskParams,
        source_path_str: &str,
        path: &Path,
    ) -> Result<bool, Error> {
        let path = path.strip_prefix(source_path_str)?;
        let mut components = path
            .components()
            .map(|c| c.as_os_str().to_str().unwrap_or(""));
        Ok(components.next().is_some_and(|c| {
            convert_params
                .whitelisted_dirs
                .iter()
                .any(|s| c.to_lowercase() == s.to_lowercase())
        }))
    }

    fn convert_file(
        message_tx: &mpsc::Sender<TaskMessage>,
        params: &OptimizeTaskParams,
        path: &Path,
        backup_path: &Path,
    ) -> Result<(bool, AudioFormat), String> {
        let source_path_str = params.source_path_as_string();
        let is_dry_run = params.is_dry_run();
        let is_verbose = params.is_verbose();
        let target_bit_depth = params.target_bit_depth;
        let target_sample_rate = params.target_sample_rate;

        if Self::is_valid_path(params, &source_path_str, &path).is_ok_and(|valid| !valid) {
            return Err("file is in an ignored directory".to_string());
        }
        let Ok(format) = analyze_audio_format(&path) else {
            return Err("could not read file as a wav".to_string());
        };
        let is_dry_run = is_dry_run;
        let convert_to_mono = params.convert_from_dual_mono_enabled
            && format.channels == 2
            && analyze_audio_is_mono(&path, &format, is_verbose).map_err(|e| e.to_string())?;
        let rel_path = path
            .strip_prefix(source_path_str)
            .map_err(|e| e.to_string())?;
        let backup_path = backup_path.join(rel_path);

        let target_bit_depth = AudioBitDepth::from_u16(target_bit_depth);
        let target_sample_rate = AudioSampleRate::from_u32(target_sample_rate);

        let do_conversion = convert_to_mono
            || target_bit_depth
                .as_ref()
                .is_some_and(|f| f.to_u16() < format.bit_depth)
            || target_sample_rate
                .as_ref()
                .is_some_and(|f| f.to_u32() < format.sample_rate);

        if do_conversion {
            if is_verbose {
                message_tx.log(format!(
                    "{}",
                    style(format!(
                        "- converted from: {} bit, {} Hz, {} channel(s)",
                        format.bit_depth, format.sample_rate, format.channels
                    ))
                    .green()
                ));
            }

            if !is_dry_run {
                std::fs::create_dir_all(
                    &backup_path
                        .parent()
                        .ok_or("Failed to get backup path parent.")?,
                )
                .map_err(|e| format!("Error creating backup directory: {}", e))?;
                std::fs::copy(path, &backup_path)
                    .map_err(|e| format!("Error backing up file: {}", e))?;
                std::fs::remove_file(path)
                    .map_err(|e| format!("Error removing original file: {}", e))?;
            }

            let ffmpeg_path = which("ffmpeg")?;

            // build ffmpeg command
            let mut command = std::process::Command::new(ffmpeg_path);
            command
                .args(["-y"])
                .args(["-i", backup_path.to_str().unwrap()]);
            if let Some(target_bit_depth) = target_bit_depth {
                command.args(["-acodec", &format!("pcm_s{}le", target_bit_depth.to_u16())]);
            }
            if let Some(target_sample_rate) = target_sample_rate {
                command.args(["-ar", &format!("{}", target_sample_rate.to_u32())]);
            }
            if convert_to_mono {
                command.args(["-ac", "1"]);
            }
            command.args([path.to_str().unwrap()]);

            command_run(&mut command, is_dry_run, is_verbose).map_err(|e| e.to_string())?;
            return Ok((true, format));
        }
        Ok((false, format))
    }
}

fn is_wav(path: &Path) -> bool {
    file_get_extension(path).is_some_and(|ext| ext == "wav")
}

fn file_get_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase())
}

fn collect_audio_files(source_path: &PathBuf) -> Vec<PathBuf> {
    crate::collect_paths(source_path)
        .into_iter()
        .filter(|p| {
            is_wav(p)
                || file_get_extension(p).is_some_and(|ext| OTHER_FORMATS.contains(&ext.as_str()))
        })
        .collect()
}

struct AudioFormat {
    bit_depth: u16,
    sample_rate: u32,
    channels: u16,
}

fn analyze_audio_is_mono(path: &Path, format: &AudioFormat, verbose: bool) -> Result<bool, Error> {
    assert!(is_wav(path));

    if format.channels == 1 {
        return Ok(true);
    }

    let ffmpeg_path = which("ffmpeg")?;
    let mut command = std::process::Command::new(ffmpeg_path);
    command
        .args(["-i", path.to_str().unwrap()])
        .args([
            "-filter_complex",
            "stereotools=phasel=1[tmp];[tmp]pan=1c|c0=0.5*c0+0.5*c1,volumedetect",
        ])
        .args(["-f", "null", "/dev/null"]);

    let result = command_run(&mut command, false, verbose)?.unwrap();

    let stderr = String::from_utf8_lossy(&result.stderr);
    let volume_re = Regex::new(r"(?<=mean_volume: )([-\d.]+)").unwrap();
    let volume: f32 = volume_re
        .find(&stderr)
        .unwrap()
        .unwrap()
        .as_str()
        .parse()
        .unwrap();

    Ok(volume < DETECT_MONO_THRESHOLD)
}

fn analyze_audio_format(path: &Path) -> Result<AudioFormat, String> {
    assert!(is_wav(path));

    let ffprobe_path = which("ffprobe")?;
    let mut command = std::process::Command::new(ffprobe_path);
    command.args(["-show_streams", path.to_str().unwrap()]);

    // println!("{}", command_to_string(&command).blue());

    let result = command
        .output()
        .map_err(|e| format!("Error running ffprobe: {}", e))?;

    if !result.status.success() {
        return Err(format!("ffprobe failed with status: {}", result.status));
    }

    let stdout = String::from_utf8_lossy(&result.stdout);

    let bit_depth_re = Regex::new(r"(?<=bits_per_sample=)(\d+)").unwrap();
    let bit_depth = bit_depth_re
        .find(&stdout)
        .unwrap()
        .unwrap()
        .as_str()
        .parse()
        .unwrap();

    let sample_rate_re = Regex::new(r"(?<=sample_rate=)(\d+)").unwrap();
    let sample_rate = sample_rate_re
        .find(&stdout)
        .unwrap()
        .unwrap()
        .as_str()
        .parse()
        .unwrap();

    let channels_re = Regex::new(r"(?<=channels=)(\d+)").unwrap();
    let channels = channels_re
        .find(&stdout)
        .unwrap()
        .unwrap()
        .as_str()
        .parse()
        .unwrap();

    Ok(AudioFormat {
        bit_depth,
        sample_rate,
        channels,
    })
}
