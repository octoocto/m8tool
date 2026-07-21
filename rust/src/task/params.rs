use anyhow::{Result, bail};
use std::path::PathBuf;

use crate::PathBufExt;

const DEFAULT_WHITELISTED_DIRS: [&str; 2] = ["samples", "packs"];

#[derive(Clone, derive_builder::Builder)]
#[builder(build_fn(validate = "Self::validate", error = "anyhow::Error"))]
pub struct Params {
    #[builder(setter(custom))]
    pub input_path: PathBuf,
    #[builder(setter(custom))]
    pub backup_path: PathBuf,
    pub is_dry_run: bool,
    pub is_verbose: bool,

    // optimize task specific params
    //
    /// A list of directories to search for sample files.
    #[builder(default = "DEFAULT_WHITELISTED_DIRS.iter().map(|s| s.to_string()).collect()")]
    pub optimize_whitelisted_dirs: Vec<String>,
    /// The target bit depth for audio files (e.g., 16, 24, 32).
    #[builder(default = "16")]
    pub target_bit_depth: u16,
    /// The target sample rate for audio files (e.g., 44100, 48000).
    #[builder(default = "44100")]
    pub target_sample_rate: u32,
    /// If enabled, converts dual mono samples to mono.
    #[builder(default = "true")]
    pub optimize_dual_mono_samples_enabled: bool,

    // shrink task specific params
    //
    /// A list of directories to search for sample files.
    #[builder(default = "DEFAULT_WHITELISTED_DIRS.iter().map(|s| s.to_string()).collect()")]
    pub shrink_whitelisted_dirs: Vec<String>,
    /// If enabled, removes common prefixes from file names during the shrink task.
    #[builder(default = "true")]
    pub remove_common_prefixes: bool,
    /// If enabled, removes common suffixes from file names during the shrink task.
    #[builder(default = "false")]
    pub remove_common_suffixes: bool,
}

impl Params {
    pub fn new(input_path: PathBuf, backup_path: PathBuf) -> ParamsBuilder {
        let mut builder = ParamsBuilder::default();
        builder.input_path(input_path).backup_path(backup_path);
        builder
    }
}

impl ParamsBuilder {
    pub fn input_path(&mut self, path: PathBuf) -> &mut Self {
        self.input_path = Some(path.with_trailing_separator());
        self
    }

    pub fn backup_path(&mut self, path: PathBuf) -> &mut Self {
        self.backup_path = Some(path.with_trailing_separator());
        self
    }

    fn validate(&self) -> Result<()> {
        let input_path = &self
            .input_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("input_path is required"))?;
        let backup_path = &self
            .backup_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("backup_path is required"))?;

        input_path.expect_dir_not_empty()?;
        backup_path.expect_dir()?;

        match self.target_bit_depth {
            None | Some(8 | 16 | 24 | 32) => {}
            _ => bail!("target bit depth must be one of: 8, 16, 24, 32"),
        }
        match self.target_sample_rate {
            None | Some(44100 | 48000) => {}
            _ => bail!("target sample rate must be one of: 44100, 48000"),
        }

        Ok(())
    }
}
