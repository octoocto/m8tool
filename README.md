# M8Tool

A collection of M8 storage maintenance tools. Includes a CLI and a GUI built with Godot.

## Download

[GitHub Releases (Windows, Linux, & MacOS)](https://github.com/octoocto/m8tool/releases/latest)

Note for MacOS users: This app won't be signed, so you may need to allow the app to run in your security settings after downloading it.

## Building/Installation

### Requirements
- Python 3.10 or newer (if building from source)
- Rust 1.97.0 or newer (if building via `cargo install`)

### Building the CLI

#### Option A: Using `cargo install`
```bash
cargo install m8tool --git https://github.com/octoocto/m8tool
```

#### Option B: Building from source
```bash
git clone https://github.com/octoocto/m8tool
cd m8tool
python build.py cli --release
```

### Building the GUI
```bash
git clone https://github.com/octoocto/m8tool
cd m8tool
python build.py gui --release
```

## CLI Usage

```bash
# usage:
m8tool [TASKS] -b <BACKUP_PATH> [-i <INPUT_PATH>] [-d/--dry-run]

# help:
m8tool --help

# find an M8 SD card and back it up to BACKUP_PATH:
m8tool backup -b <BACKUP_PATH>

# find an M8 SD card and optimize all samples on it (originals saved to BACKUP_PATH):
m8tool optimize -b <BACKUP_PATH>

# find an M8 SD card and shrink paths to samples (copies of affected samples as their original paths saved to BACKUP_PATH):
m8tool shrink -b <BACKUP_PATH>

# find an M8 SD card and clean all extra files (removed files saved to BACKUP_PATH):
m8tool clean -b <BACKUP_PATH>

# provide an explicit path to an SD card, perform a backup, then clean:
m8tool backup clean -i <INPUT_PATH> -b <BACKUP_PATH>

# run a default set of tasks (by default this is just "backup")
m8tool -b <BACKUP_PATH>
```

## GUI Usage

1. **Choose a source drive to perform tasks on.** This is either the SD card of the M8 or the M8 itself in USB drive mode. Optionally, you can also choose any directory to use as the source drive.
2. **Choose a backup directory.** Every individual task in M8tool creates a separate backup of any original files that the task removes or modifies.
3. **Choose which task(s) to enable.** The enabled task(s) will be ran in order of top-to-bottom. It is recommended to enable the "full backup" task if any other task is also enabled.


## Tasks

### `backup` 
Creates a full backup of the SD card (or `INPUT_PATH`) to `BACKUP_PATH` using `rsync`. Based off laamaa's M8 backup script.

**I personally recommend always running this task first before doing any of the other tasks.**

### `clean`
Removes all extra files from the SD card (or `INPUT_PATH`). Files that were removed are backed up to `BACKUP_PATH`.

Removed files include:
- hidden files and dotfiles/folders, such as those created by the OS (`__MACOSX`, `.DS_Store`, etc.)
- any file that the M8 does not read (anything that is NOT a `.m8t, .m8s, .m8i, .m8n, .wav`)

### `optimize` (requires `ffmpeg` and `ffprobe`)
Modifies samples on the SD card (or `INPUT_PATH`) to an optimal format. Affected samples are backed up to `BACKUP_PATH`.

By default, only modifies samples in the `Samples/` and `Packs/` subfolders.

By default, modifies samples to have a maximum sample rate/bit depth of `44.1kHz/16-bit`. 

### `shrink`
Shrinks the names of directories and samples on the SD card (or `INPUT_PATH`) in order to minimize the length of paths to samples.

Copies of affected samples in their original directory/name are backed up to `BACKUP_PATH`.

## Project Structure

`rust/`: Source code for the M8tool CLI and GDExtension for Godot.

`godot/`: Godot project files for the M8tool GUI.
