use crate::FileStatus;
use crate::PathBufExt;
use crate::audio::AudioBitDepth;
use crate::audio::AudioFormat;
use crate::audio::AudioSampleRate;
use crate::audio::read_format;
use crate::command_run;
use crate::task;
use crate::which;
use anyhow::{Result, bail};
use console::style;
use std::path::{Path, PathBuf};

// if enabled, will convert other formats to wav
const OTHER_FORMATS: [&str; 5] = ["mp3", "ogg", "flac", "aiff", "aif"];

// if enabled, will try to detect if stereo files are actually mono and convert them
// const DETECT_MONO_THRESHOLD: f32 = -90.0;

#[derive(Clone)]
pub struct OptimizeTask;

impl task::Task for OptimizeTask {
    fn name(&self) -> &str {
        "optimize"
    }

    fn start_message(&self) -> &str {
        "optimizing samples..."
    }

    fn finish_message(&self, result: &task::TaskResult) -> String {
        let processed = style(result.paths_processed.len()).bold();
        let modified = style(result.paths_modified.len()).bold();
        let p = style(result.task_backup_path.to_string()).bold();
        format!("{modified}/{processed} samples optimized. originals backed up to {p}")
    }

    fn collect_paths(&self, params: &task::Params) -> Vec<PathBuf> {
        collect_audio_files(&params.input_path)
    }

    fn spawn(&self, params: &task::Params) -> Result<task::TaskProcess> {
        Ok(task::TaskProcess::new(self, params))
    }

    fn thread_fn(&self, params: &task::Params) -> task::TaskFn {
        let params = params.clone();
        let func = move |mut handle: task::TaskProcessHandle| {
            let mut processed_paths = vec![];
            let mut converted_paths = vec![];
            // let mut num_processed = 0;
            let mut num_skipped = 0;
            let mut num_good = 0;

            handle.logva(&[
                style(format!("starting task \"{}\"...", Self.name()))
                    .green()
                    .to_string(),
                format!("ffmpeg: {}", which("ffmpeg")?.display()),
                format!("ffprobe: {}", which("ffprobe")?.display()),
                format!(
                    "whitelisted dirs: {}",
                    params.optimize_whitelisted_dirs.join(", ")
                ),
                format!("target bit depth: {} bit", params.target_bit_depth).to_string(),
                format!("target sample rate: {} Hz", params.target_sample_rate).to_string(),
                format!(
                    "convert from dual mono enabled: {}",
                    params.optimize_dual_mono_samples_enabled
                ),
                format!("total files to process: {}", handle.paths().len()),
            ]);

            // optimize wavs
            for file_path in handle.paths().clone() {
                if !handle.should_run() {
                    return Ok(handle.interrupted(processed_paths, converted_paths));
                }

                if file_path.is_wav_file() {
                    let (status, audio_format) =
                        match Self::convert_file(&params, &mut handle, &file_path) {
                            Ok((was_converted, audio_format)) => {
                                if was_converted {
                                    converted_paths.push(file_path.clone());
                                    (FileStatus::Changed, Some(audio_format))
                                } else {
                                    num_good += 1;
                                    (FileStatus::Unchanged, Some(audio_format))
                                }
                            }
                            Err(e) => {
                                num_skipped += 1;
                                (FileStatus::Skipped(e.to_string()), None)
                            }
                        };

                    processed_paths.push(file_path.clone());

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

                    let path_relative = file_path
                        .strip_prefix(&params.input_path)
                        .unwrap_or(&file_path);

                    handle.send_progress_with_meta(
                        Self.name(),
                        format!("{}", path_relative.display()),
                        processed_paths.len(),
                        handle.paths().len(),
                        status,
                        audio_format_meta,
                    );
                } else if let Some(ext) = file_path.get_extension() {
                    // convert other samples to wav
                    if OTHER_FORMATS.contains(&ext.as_str()) {
                        handle.log(style(format!("found: {}", file_path.display())).yellow());
                    }
                }
            }

            handle.log(
                style(format!(
                    "processed {} files ({} converted, {} good, {} skipped)",
                    processed_paths.len(),
                    converted_paths.len(),
                    num_good,
                    num_skipped,
                ))
                .green(),
            );
            handle.log(
                style(format!(
                    "backup of original files has been made in: {}",
                    handle.task_backup_path().display()
                ))
                .green(),
            );

            Ok(handle.finished(processed_paths, converted_paths))
        };

        Box::new(func)
    }
}

impl OptimizeTask {
    /// Checks if the path is in a whitelisted directory.
    fn is_valid_path(
        convert_params: &task::Params,
        source_path_str: &PathBuf,
        path: &Path,
    ) -> Result<bool> {
        let path = path.strip_prefix(source_path_str)?;
        let mut components = path
            .components()
            .map(|c| c.as_os_str().to_str().unwrap_or(""));
        Ok(components.next().is_some_and(|c| {
            convert_params
                .optimize_whitelisted_dirs
                .iter()
                .any(|s| c.to_lowercase() == s.to_lowercase())
        }))
    }

    fn convert_file(
        params: &task::Params,
        handle: &mut task::TaskProcessHandle,
        file_path: &Path,
    ) -> Result<(bool, AudioFormat)> {
        let is_dry_run = params.is_dry_run;
        let target_bit_depth = params.target_bit_depth;
        let target_sample_rate = params.target_sample_rate;

        if Self::is_valid_path(params, &params.input_path, &file_path).is_ok_and(|valid| !valid) {
            bail!("file is in an ignored directory");
        }
        let format = read_format(&file_path, params.optimize_dual_mono_samples_enabled)?;
        let is_dry_run = is_dry_run;
        let convert_to_mono = format.is_dual_mono.unwrap_or(false);
        let rel_path = file_path.strip_prefix(&params.input_path)?;
        let backup_path = handle.task_backup_path().join(rel_path);

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
            if convert_to_mono {
                handle.logv(style("- sample is dual mono").green());
            }
            handle.logv(format!(
                "{}",
                style(format!(
                    "- converted from: {} bit, {} Hz, {} channel(s)",
                    format.bit_depth, format.sample_rate, format.channels
                ))
                .green()
            ));

            if !is_dry_run {
                crate::remove_and_backup_file(file_path, &backup_path)?;
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
            command.args([file_path.to_str().unwrap()]);

            command_run(&mut command, is_dry_run, false)?;
            return Ok((true, format));
        }
        Ok((false, format))
    }
}

fn collect_audio_files(source_path: &PathBuf) -> Vec<PathBuf> {
    crate::collect_paths(source_path)
        .into_iter()
        .filter(|p| {
            p.is_wav_file()
                || p.get_extension()
                    .is_some_and(|ext| OTHER_FORMATS.contains(&ext.as_str()))
        })
        .collect()
}
