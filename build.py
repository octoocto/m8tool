#!/usr/bin/env python3

import argparse
import os
import platform
import shlex
import shutil
import ssl
import stat
import subprocess
import sys
import time
import urllib.error
import urllib.request
import zipfile
from pathlib import Path
from typing import Literal

BIN_NAME = "m8tool"
BUILD_DIR = "build"
# LIB_OUT_DIR = "project/addons/lib%s" % PROJECT_NAME
GODOT_VERSION = "4.7"
GODOT_BRANCH = "stable"
################################################################################
# Functions/Variables

godot_url_root = f"https://github.com/godotengine/godot/releases/download/{GODOT_VERSION}-{GODOT_BRANCH}/"
godot_zip_export_templates = (
    f"Godot_v{GODOT_VERSION}-{GODOT_BRANCH}_export_templates.tpz"
)
godot_zip_win = f"Godot_v{GODOT_VERSION}-{GODOT_BRANCH}_win64.exe.zip"
godot_zip_linux = f"Godot_v{GODOT_VERSION}-{GODOT_BRANCH}_linux.x86_64.zip"
godot_zip_mac = f"Godot_v{GODOT_VERSION}-{GODOT_BRANCH}_macos.universal.zip"
godot_path_win = f"Godot_v{GODOT_VERSION}-{GODOT_BRANCH}_win64.exe"
godot_path_linux = f"Godot_v{GODOT_VERSION}-{GODOT_BRANCH}_linux.x86_64"
godot_path_mac = "Godot.app"

Platform = Literal["windows", "linux", "macos"]

################################################################################
# Argument Parser

parser = argparse.ArgumentParser()
_ = parser.add_argument(
    "target",
    type=str,
    choices=["cli", "gui"],
    default="cli",
    help="set the target build type (cli or gui)",
)
_ = parser.add_argument(
    "--release",
    action="store_true",
    help=f"build the release release version of {BIN_NAME}",
)
_ = parser.add_argument(
    "--extension",
    action="store_true",
    help=f"only build the lib{BIN_NAME} GDExtension (does not export {BIN_NAME})",
)
_ = parser.add_argument(
    "--platform",
    type=str,
    choices=["windows", "linux", "macos"],
    default="",
    help="set the target platform to build for",
)
_ = parser.add_argument(
    "--nodownload",
    action="store_true",
    help="run this script without downloading anything",
)


def _print(text: str) -> None:
    print(f"\033[92m{text}\033[0m", end="")


def _println(text: str) -> None:
    print(f"\033[92m:: {text}\033[0m", flush=True)


def _println_info(text: str) -> None:
    print(f"\033[34m{text}\033[0m", flush=True)


def _println_err(text: str) -> None:
    print(f"\033[91m{text}\033[0m")


def get_export_templates_path() -> str:
    match platform.system():
        case "Windows":
            return os.path.expandvars(
                f"%APPDATA%\\Godot\\export_templates\\{GODOT_VERSION}.stable"
            )
        case "Linux":
            return os.path.expanduser(
                f"~/.local/share/godot/export_templates/{GODOT_VERSION}.stable"
            )
        case "MacOS" | "Darwin":
            return os.path.expanduser(
                f"~/Library/Application Support/Godot/export_templates/{GODOT_VERSION}.stable"
            )
        case _:
            raise OSError()


def using_cygwin() -> bool:
    return bool(shutil.which("cygpath"))


def find_godot() -> str:
    path = which("godot")

    if path != None:
        _println(f"found! {path}")
        return path

    if platform.system() == "Windows":
        file_path = Path(f"{BUILD_DIR}/{godot_path_win}")
    elif platform.system() == "Linux":
        file_path = Path(f"{BUILD_DIR}/{godot_path_linux}")
    elif platform.system() == "Darwin":  # MacOS
        file_path = Path(f"{BUILD_DIR}/{godot_path_mac}/Contents/MacOS/Godot")
    else:
        raise RuntimeError("Unsupported platform!")

    if file_path.exists():
        file_path.chmod(file_path.stat().st_mode | stat.S_IEXEC)
        path = which(file_path.as_posix())
        if path != None:
            return path

    raise RuntimeError(f"Could not find godot in {file_path}!")


def chmod_x(path: str) -> None:
    file_path = Path(path)
    file_path.chmod(file_path.stat().st_mode | stat.S_IEXEC)


def find_command(cmd: str) -> str:
    path: str | None = which(cmd)
    if path != None:
        _println(f"Found {cmd}! ({path})")
        return path
    else:
        _println_err(f"Could not find {cmd}!")
        sys.exit(1)


def find_bash() -> str:
    path: str | None = shutil.which("bash")
    if path != None:
        return path
    else:
        _println_err("Could not find bash!")
        sys.exit(1)


def which(path: str) -> str | None:
    if not using_cygwin():
        return shutil.which(path)
    else:
        which_path = shutil.which("which")
        if which_path != None:
            try:
                return subprocess.check_output([which_path, path]).decode().strip()
            except subprocess.CalledProcessError:
                pass


def exec_and_capture(
    command: str,
    cwd: str | None = None,
    env: None = None,
    *,
    capture_output: bool = False,
):
    old_cwd = os.getcwd()
    if cwd:
        _println_info(f"cd {cwd}")
        os.chdir(cwd)

    _println_info(command)

    args = []
    result = None
    if not using_cygwin():
        args = shlex.split(command)
    else:
        bash_path = find_bash()
        if bash_path:
            args = [bash_path, "--login", "-c", command]

    result = subprocess.run(args, check=False, env=env, capture_output=capture_output)

    # restore working directory
    if cwd:
        _println_info(f"cd {old_cwd}")
        os.chdir(old_cwd)

    returncode = result.returncode

    if returncode != 0:
        if result and result.stderr:
            print(result.stderr.decode())
        raise RuntimeError(
            f"Command {command} returned non-zero exit status: {returncode}"
        )

    return result


def exec(command: str, cwd: str | None = None, env: None = None) -> None:
    _ = exec_and_capture(command, cwd, env, capture_output=False)


def copy_to_build_dir(file_path: str, subfolder: str = "") -> None:
    build_path = Path(BUILD_DIR) / subfolder
    build_path.mkdir(parents=True, exist_ok=True)

    file_name = Path(file_path).name
    dest_path = build_path / file_name

    # file_dir = Path(file_path).parent
    # _println("Files in directory %s:" % file_dir)
    # for file in [f for f in Path(file_dir).iterdir() if f.is_file()]:
    #     _println("- %s" % file.name)

    _ = shutil.copy(file_path, dest_path)
    _println(f"Copied {file_name} to {build_path}")


def copy(file_path: str, dest_path: str) -> None:
    Path(dest_path).parent.mkdir(parents=True, exist_ok=True)
    _ = shutil.copy(file_path, dest_path)
    _println(f"Copied {file_path} to {dest_path}")


def exec_cargo_build(
    build_gdext: bool,
    is_release: bool,
    platform: str,
    cargo_targets: list[str],
) -> None:
    cwd = "rust"
    cargo = find_command("cargo")
    rustup = find_command("rustup")
    release_or_debug = "release" if is_release else "debug"
    cargo_flags = ""
    if is_release:
        cargo_flags += "--release "
    if build_gdext:
        cargo_flags += "--features gdext --lib"

    exec(f"{cargo} --version")
    exec(f"{rustup} --version")

    for target in cargo_targets:
        exec(f"{rustup} target add {target}")
        exec(f"{cargo} build {cargo_flags} --target {target}", cwd)

    match platform:
        case "macos":
            if build_gdext:
                filename = f"lib{BIN_NAME}.dylib"
            else:
                filename = f"{BIN_NAME}"

            _println("Creating universal binary for macOS...")
            file_x86 = f"{cwd}/target/x86_64-apple-darwin/{release_or_debug}/{filename}"
            file_arm = (
                f"{cwd}/target/aarch64-apple-darwin/{release_or_debug}/{filename}"
            )
            file_uni = f"{cwd}/target/{release_or_debug}/{filename}"
            exec(f"lipo -create {file_x86} {file_arm} -output {file_uni}")

            if not build_gdext:
                copy_to_build_dir(file_uni, "cli")

        case "windows":
            if build_gdext:
                filename = f"{BIN_NAME}.dll"
            else:
                filename = f"{BIN_NAME}.exe"

            filepath = (
                f"{cwd}/target/x86_64-pc-windows-gnu/{release_or_debug}/{filename}"
            )

            copy(filepath, f"{cwd}/target/{release_or_debug}/{filename}")
            if not build_gdext:
                copy_to_build_dir(filepath, "cli")

        case "linux":
            if build_gdext:
                filename = f"lib{BIN_NAME}.so"
            else:
                filename = f"{BIN_NAME}"

            filepath = (
                f"{cwd}/target/x86_64-unknown-linux-gnu/{release_or_debug}/{filename}"
            )

            copy(filepath, f"{cwd}/target/{release_or_debug}/{filename}")
            if not build_gdext:
                copy_to_build_dir(filepath, "cli")

        case _:
            raise RuntimeError(f"Unsupported platform: {platform}")


def download_zip(url: str, dest_dir: str) -> None:
    _println(f"Downloading {url}...")
    ssl._create_default_https_context = ssl._create_unverified_context
    res = urllib.request.urlretrieve(url)
    with zipfile.ZipFile(res[0], "r") as zip:
        zip.extractall(dest_dir)
        _println(f"Extracted zip to {dest_dir}...")


def format_platform(p: str) -> Platform:
    p = p.lower()
    if p == "":
        p = platform.system().lower()
    if p == "darwin":
        p = "macos"
    match p:
        case "windows":
            return "windows"
        case "linux":
            return "linux"
        case "macos":
            return "macos"
        case _:
            _println_err(f"Unsupported platform: {p}")
            sys.exit(1)


################################################################################
# Build Script

args = parser.parse_args()
is_cli = bool(args.target == "cli")  # pyright: ignore[reportAny]
is_release = bool(args.release)  # pyright: ignore[reportAny]
is_extension_only = bool(args.extension)  # pyright: ignore[reportAny]
is_nodownload = bool(args.nodownload)  # pyright: ignore[reportAny]
target_platform = format_platform(str(args.platform))  # pyright: ignore[reportAny]

if is_cli:
    build_cli = True
    build_gdext = False
    build_export = False
elif is_extension_only:
    build_cli = False
    build_gdext = True
    build_export = False
else:
    build_cli = False
    build_gdext = True
    build_export = True


_println(f"Building for {target_platform} platform...")

match target_platform:
    case "macos":
        app_extension = ".app"
        cargo_targets = ["x86_64-apple-darwin", "aarch64-apple-darwin"]
    case "windows":
        app_extension = ".exe"
        cargo_targets = ["x86_64-pc-windows-gnu"]
        # _ = os.system("color")
    case "linux":
        app_extension = ".x86_64"
        cargo_targets = ["x86_64-unknown-linux-gnu"]

if is_release:
    godot_target = "--export-release"
else:
    godot_target = "--export-debug"

# create build directory if doesn't exist
build_path = Path(BUILD_DIR)
build_path.mkdir(exist_ok=True)

exec_cargo_build(build_gdext, is_release, target_platform, cargo_targets)

if not build_export:
    _println("Done!")
    sys.exit(0)

# find or download Godot export templates

try:
    godot_path = find_godot()
    found_godot = True
    _println("Found godot!")
except RuntimeError:
    godot_path = None
    found_godot = False

export_templates_path = Path(get_export_templates_path())
if (
    found_godot
    and export_templates_path.is_dir()
    and any(export_templates_path.iterdir())
):
    found_godot_templates = True
    _println("Found export templates!")
else:
    found_godot_templates = False

if not found_godot_templates:
    if is_nodownload:
        _println_err("Could not find export templates!")
        _println("Download required to continue, but found --nodownload flag. Exiting.")
        sys.exit(1)

    url: str = f"{godot_url_root}{godot_zip_export_templates}"

    download_zip(url, BUILD_DIR)
    # move templates
    if found_godot:
        shutil.move(f"{BUILD_DIR}/templates", export_templates_path)
    else:
        shutil.move(
            f"{BUILD_DIR}/templates/",
            f"{BUILD_DIR}/editor_data/export_templates/{GODOT_VERSION}.{GODOT_BRANCH}/",
        )

# find or download Godot editor

if not found_godot:
    if is_nodownload:
        _println_err("Could not find godot!")
        _println("Download required to continue, but found --nodownload flag. Exiting.")
        sys.exit(1)

    match target_platform:
        case "windows":
            url = f"{godot_url_root}{godot_zip_win}"
        case "linux":
            url = f"{godot_url_root}{godot_zip_linux}"
        case "macos":
            url = f"{godot_url_root}{godot_zip_mac}"

    download_zip(url, BUILD_DIR)
    Path(f"{BUILD_DIR}/_sc_").touch()
    godot_path = find_godot()

# export the Godot project

_println(f"Exporting Godot project for {target_platform} platform...")
Path(f"{BUILD_DIR}/gui").mkdir(parents=True, exist_ok=True)
exec(
    f"{godot_path} --headless --path godot {godot_target} {target_platform} ../{BUILD_DIR}/gui/{BIN_NAME}{app_extension}"
)
_println('Done! The exported app will be found in the "build" folder.')
sys.exit(0)
