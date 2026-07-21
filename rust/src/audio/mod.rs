use anyhow::{Error, Result, anyhow, bail};
use itertools::Itertools;
use std::path::Path;

#[derive(Debug)]
pub enum AudioBitDepth {
    Bit8,
    Bit16,
    Bit24,
    Bit32,
}

impl AudioBitDepth {
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            8 => Some(Self::Bit8),
            16 => Some(Self::Bit16),
            24 => Some(Self::Bit24),
            32 => Some(Self::Bit32),
            _ => None,
        }
    }

    pub fn to_u16(&self) -> u16 {
        match self {
            Self::Bit8 => 8,
            Self::Bit16 => 16,
            Self::Bit24 => 24,
            Self::Bit32 => 32,
        }
    }
}

#[derive(Debug)]
pub enum AudioSampleRate {
    Hz44100,
    Hz48000,
}

impl AudioSampleRate {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            44100 => Some(Self::Hz44100),
            48000 => Some(Self::Hz48000),
            _ => None,
        }
    }

    pub fn to_u32(&self) -> u32 {
        match self {
            Self::Hz44100 => 44100,
            Self::Hz48000 => 48000,
        }
    }
}

pub struct AudioFormat {
    pub bit_depth: u16,
    pub sample_rate: u32,
    pub channels: u16,
    pub is_dual_mono: Option<bool>,
}

type WavReader = hound::WavReader<std::io::BufReader<std::fs::File>>;

pub fn read_format(path: &Path, check_dual_mono: bool) -> Result<AudioFormat, Error> {
    match hound::WavReader::open(path) {
        Ok(mut reader) => {
            let spec = reader.spec();
            Ok(AudioFormat {
                bit_depth: spec.bits_per_sample,
                sample_rate: spec.sample_rate,
                channels: spec.channels,
                is_dual_mono: if check_dual_mono {
                    Some(read_and_check_dual_mono(&mut reader, &spec)?)
                } else {
                    None
                },
            })
        }
        Err(e) => {
            if which::which("ffprobe").is_err() {
                return Err(e.into());
            }
            read_format_ffprobe(path)
        }
    }
}

fn read_and_check_dual_mono(reader: &mut WavReader, spec: &hound::WavSpec) -> Result<bool, Error> {
    if spec.channels == 1 || spec.channels > 2 {
        return Ok(false);
    }
    if spec.bits_per_sample == 8 {
        return is_dual_mono::<i8>(reader);
    } else if spec.bits_per_sample == 16 {
        return is_dual_mono::<i16>(reader);
    } else if spec.bits_per_sample == 24 {
        return is_dual_mono::<i32>(reader);
    } else if spec.bits_per_sample == 32 {
        return is_dual_mono::<i32>(reader);
    } else {
        return Ok(false);
    }
}

fn is_dual_mono<T: hound::Sample + PartialEq + Default>(
    reader: &mut hound::WavReader<std::io::BufReader<std::fs::File>>,
) -> Result<bool, Error> {
    let is_mono = reader
        .samples::<T>()
        .tuples()
        .all(|(left, right)| left.unwrap_or_default() == right.unwrap_or_default());
    Ok(is_mono)
}

fn read_format_ffprobe(path: &Path) -> Result<AudioFormat, Error> {
    let output = std::process::Command::new("ffprobe")
        .arg("-v")
        .arg("quiet")
        .arg("-print_format")
        .arg("flat")
        .arg("-show_streams")
        .arg(path)
        .output()?;

    if !output.status.success() {
        bail!("ffprobe failed with status {}", output.status);
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    let mut lines = output_str.lines();
    let (mut sample_rate, mut channels, mut bit_depth) = (None, None, None);

    for line in &mut lines {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| anyhow!("Invalid ffprobe output"))?;

        match key {
            "streams.stream.0.sample_rate" => {
                sample_rate = value.trim_matches('"').parse::<u32>().ok();
            }
            "streams.stream.0.channels" => {
                channels = value.trim_matches('"').parse::<u16>().ok();
            }
            "streams.stream.0.bits_per_sample" => {
                bit_depth = value.trim_matches('"').parse::<u16>().ok();
            }
            _ => {}
        }
    }

    if sample_rate.is_none() || channels.is_none() || bit_depth.is_none() {
        bail!("Missing audio format information from ffprobe output");
    }

    Ok(AudioFormat {
        bit_depth: bit_depth.unwrap(),
        sample_rate: sample_rate.unwrap(),
        channels: channels.unwrap(),
        is_dual_mono: None, // ffprobe does not provide dual mono info
    })
}
