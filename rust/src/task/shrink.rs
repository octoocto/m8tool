use crate::PathBufExt;
use crate::is_path_in_dir_whitelist;
use crate::task::*;
use anyhow::Result;
use console::style;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const TASK_NAME: &str = "shrink";

/// Characters to be replaced with space
const SPLIT_CHARS: &[char] = &['-', '_', ' ', '+', '.', '(', ')', '[', ']'];

/// Characters to be removed
const FILL_CHARS: &[char] = &[',', '\'', '#'];

/// A task that minimizes paths to samples by renaming directories and file names.
/// A backup of the original paths is created in the backup directory.
#[derive(Clone)]
pub struct ShrinkTask;

impl Task for ShrinkTask {
    fn name(&self) -> &str {
        "shrink"
    }

    fn start_message(&self) -> &str {
        "shrinking paths to samples..."
    }

    fn finish_message(&self, result: &TaskResult) -> String {
        let n = style(result.paths_modified.len()).bold();
        let p = style(result.task_backup_path.to_string()).bold();
        format!("{n} sample paths renamed. originals backed up to {p}")
    }

    fn collect_paths(&self, params: &Params) -> Vec<PathBuf> {
        let source_path = &params.input_path;
        let whitelisted_dirs = &params.shrink_whitelisted_dirs;
        crate::collect_paths(source_path)
            .into_iter()
            .filter(|path| path.is_file() && path.has_extension("wav"))
            .filter(|path| {
                if let Ok(path) = path.strip_prefix(source_path) {
                    is_path_in_dir_whitelist(path, whitelisted_dirs).unwrap_or(false)
                } else {
                    false
                }
            })
            .collect()
    }

    fn spawn(&self, params: &Params) -> Result<TaskProcess> {
        Ok(TaskProcess::new(self, params))
    }

    fn thread_fn(&self, params: &Params) -> TaskFn {
        let source_path = params.input_path.clone();
        let is_dry_run = params.is_dry_run;
        let params = params.clone();
        let name = self.name().to_owned();

        let func = move |mut handle: TaskProcessHandle| {
            handle.logva(&[
                style(format!(
                    "whitelisted dirs: {:?}",
                    params.shrink_whitelisted_dirs
                ))
                .green(),
                style(format!(
                    "remove common prefixes: {}",
                    params.remove_common_prefixes
                ))
                .green(),
                style(format!(
                    "remove common suffixes: {}",
                    params.remove_common_suffixes
                ))
                .green(),
                style(format!("starting task \"{}\"...", name)).green(),
            ]);

            let mut processed_paths = vec![];
            let mut renamed_paths = vec![];

            for path in handle.paths() {
                if !handle.should_run() {
                    return Ok(handle.interrupted(processed_paths, renamed_paths));
                }
                let relative_path = path.strip_prefix(&source_path)?;
                let minimized_path =
                    shrink_path(&params, &source_path, &relative_path.to_path_buf())?;
                let backup_file_path = handle.task_backup_path().join(&relative_path);
                let new_file_path = source_path.join(&minimized_path);

                if let Some(relative_path) = relative_path.to_str()
                    && let Some(minimized_path) = minimized_path.to_str()
                    && minimized_path.len() < relative_path.len()
                    && !new_file_path.exists()
                {
                    handle.log(style(path.strip_prefix(&source_path)?.display()));
                    handle.log(style(new_file_path.strip_prefix(&source_path)?.display()).green());

                    if !is_dry_run {
                        std::fs::create_dir_all(backup_file_path.parent().unwrap())?;
                        std::fs::copy(&path, &backup_file_path)?;
                        std::fs::rename(&path, &new_file_path)?;
                    }

                    renamed_paths.push(path.clone());
                }
                processed_paths.push(path.clone());

                handle.send_progress(
                    TASK_NAME,
                    relative_path.to_string_lossy().to_string(),
                    processed_paths.len(),
                    handle.paths().len(),
                );
            }

            handle.loga(&[
                style(format!(
                    "processed {} files ({} renamed)",
                    processed_paths.len(),
                    renamed_paths.len(),
                ))
                .green(),
                style(format!(
                    "backup of original files has been made in: {}",
                    handle.task_backup_path().display()
                ))
                .green(),
            ]);

            Ok(handle.finished(processed_paths, renamed_paths))
        };

        Box::new(func)
    }
}

/// Shrinks a path name as much as possible. This will:
/// - Remove any common punctuation characters (e.g. "-", "_", " ", etc.)
/// - Remove any duplicate words on the path (e.g. "samples/samples/sample.wav" ->
/// "samples/sample.wav")
fn shrink_path(params: &Params, input_path: &PathBuf, path: &PathBuf) -> Result<PathBuf> {
    path.expect_relative()?;
    path.expect_extension("wav")?;

    let mut unique_words: HashSet<String> = HashSet::new();
    let base_dir = path.base_dir()?;
    let mut file_stem = path.file_stem()?;
    let is_dotfile = path.is_dotfile();
    let remove_common_prefixes = params.remove_common_prefixes;

    // shrink each part of the path separately, and filter out empty parts
    //
    let dir_parts = base_dir.components().filter_map(|dir_part| {
        let dir_part = dir_part.as_os_str().to_string_lossy().to_string();
        let dir_part = _shrink_part(&dir_part, &mut unique_words);
        if dir_part.is_empty() {
            None
        } else {
            Some(dir_part)
        }
    });

    let new_base_dir = PathBuf::from_iter(dir_parts);

    // remove common prefix from file stem

    if remove_common_prefixes {
        if let Some(common_prefix) = get_common_prefix_in_path(&input_path.join(path))
            && let Some(stripped) = file_stem.strip_prefix(&common_prefix)
        {
            file_stem = stripped.to_string();
        }
    }

    // minimize file stem

    file_stem = _shrink_part(&file_stem, &mut unique_words);
    if is_dotfile {
        file_stem = ".".to_string() + &file_stem;
    }

    // println!(
    //     "unique prefix: {:?}",
    //     get_common_prefix_in_path(&source_path.join(path))
    // );

    Ok(new_base_dir.join(file_stem + ".wav"))
}

fn _shrink_part(part: &str, unique_words: &mut HashSet<String>) -> String {
    let mut part = part.to_string();
    // replace split chars with space, and remove fill chars
    for c in SPLIT_CHARS {
        part = part.replace(*c, " ");
    }
    for c in FILL_CHARS {
        part = part.replace(*c, "");
    }
    let words = part.split_whitespace().collect::<Vec<&str>>();

    let words: Vec<String> = words
        .iter()
        .filter_map(|word| {
            let word = word.to_string();
            // filter out words that are in unique_words, unless they are numbers
            if word.parse::<u32>().is_ok() {
                return Some(word);
            }
            if !unique_words.contains(&word.to_lowercase()) {
                return Some(word);
            }
            None
        })
        .filter(|w| !w.is_empty())
        .collect::<Vec<String>>();

    for word in words.clone() {
        unique_words.insert(word.to_lowercase());
        // also add plural or singular form of every word
        let other = if word.to_lowercase().ends_with('s') {
            word[..word.len() - 1].to_lowercase()
        } else {
            format!("{}s", word).to_lowercase()
        };
        unique_words.insert(other);
    }

    words.join(" ")
}

/// Get the common prefix of all .wav files in a path.
fn get_common_prefix_in_path(path: &Path) -> Option<String> {
    let base_dir = if path.is_file() { path.parent()? } else { path };
    if let Err(e) = base_dir.to_path_buf().expect_dir() {
        println!("{}", e.to_string());
        return None;
    }

    let file_stems = WalkDir::new(base_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext.to_string_lossy().to_lowercase() == "wav")
        })
        .filter_map(|entry| entry.path().to_path_buf().file_stem().ok())
        .collect::<Vec<String>>();

    let file_stems = file_stems.iter().map(String::as_str).collect();

    // println!("file names: {:?}", file_names);

    common_prefix(file_stems)
}

/// Get the common prefix of a vector of strings, ignoring any numeric digits.
pub fn common_prefix(strings: Vec<&str>) -> Option<String> {
    if strings.is_empty() {
        return None;
    }
    let mut prefix = strings[0]
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .to_string();

    if prefix.is_empty() || prefix.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    // println!("starting prefix: {}", prefix);
    for s in strings.iter() {
        while !s.starts_with(&prefix) {
            if prefix.is_empty() {
                return None;
            }
            prefix.pop();
        }
    }
    if prefix.is_empty() {
        None
    } else {
        Some(prefix)
    }
}

/// Get the common suffix of a vector of strings.
// pub fn common_suffix(_strings: Vec<&str>) -> Option<String> {
//     todo!()
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_prefix() {
        assert_eq!(
            common_prefix(vec!["kick_01", "kick_02", "kick_03"]),
            Some("kick_".to_string())
        );
        assert_eq!(common_prefix(vec!["01_a", "02_a", "03_a"]), None);
        assert_eq!(common_prefix(vec!["a", "b", "c"]), None);
    }

    // #[test]
    // fn test_common_suffix() {
    //     assert_eq!(common_suffix(vec!["kick_01", "kick_02", "kick_03"]), None);
    //     assert_eq!(
    //         common_suffix(vec!["01_a", "02_a", "03_a"]),
    //         Some("_a".to_string())
    //     );
    //     assert_eq!(common_suffix(vec!["a", "b", "c"]), None);
    //     assert_eq!(
    //         common_suffix(vec!["a_10", "b_20", "c_20"]),
    //         Some("0".to_string())
    //     );
    // }
}
