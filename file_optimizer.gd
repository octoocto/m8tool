class_name FileOptimizer

signal scan_started
signal scan_finished

# queue of files to add to the tree
var item_copied_queue: Array[File]
var item_converted_queue: Array[File]
var item_skipped_queue: Array[File]

var num_files_total := 0
var num_files_scanned := 0
var num_files_copied := 0
var num_files_converted := 0
var num_files_skipped := 0

## Contains all files in [dir_in].
var dir_in_files: PackedStringArray

## Becomes populated with output file paths as files are being processed.
## Used to detect potential overwrites.
var paths_out: PackedStringArray

var shell: Shell = preload("res://shell.gd").new()

## The current file being processed.
var current_file: File
var _is_scan_in_progress := false

var dir_in := ""
var dir_out := ""

var skip_patterns: Array = ["*.txt", "*__MACOSX*", "*.DS_Store", ]
var skip_pattern_and := false
var ignore_patterns: Array

var converter_enabled := true
var converter_bit_depth_enabled := true
var converter_sample_rate_enabled := true
var converter_auto_mono_enabled := true
var converter_other_formats_enabled := false

var converter_other_formats := ["mp3", "flac"]
var converter_target_bit_depth := 16
var converter_target_sample_rate := 44100
var converter_auto_mono_threshold := -90.0

var renamer_remove_common_prefixes_enabled := true
var renamer_remove_common_suffixes_enabled := false
var renamer_minimize_path_enabled := true
var renamer_minimize_path_keep_numbers := false
var renamer_minimize_path_mode := 1
var renamer_uppercase_enabled := true
var renamer_remove_from_blacklist_enabled := false
var renamer_replace_spaces_enabled := true

# chars to be replaced with space
var renamer_split_chars: Array[String] = ["_", "-", " ", "+", ".", "(", ")", "[", "]"]

# chars to be removed
var renamer_fill_chars: Array[String] = [",", "'", "#"]

var renamer_prefix_blacklist: Array[String] = []
    # "final", "sample", "label", "process", "edit", "pack", "wav", "construct", "cpa", "splice", "export"
# ]

var renamer_replace_spaces_char := "_"

var _mutex := Mutex.new()
var _scan_thread := Thread.new()
var _stop_requested := false


##
## Return the progress of a scan as a value between [0.0] and [100.0].
##
func get_scan_progress() -> float:
    return num_files_scanned / float(num_files_total) * 100.0

func is_dir_in_valid() -> bool:
    return dir_in != ""

func is_dir_in_and_out_valid() -> bool:
    return dir_in != "" and dir_out != "" and not _path_is_subpath(dir_in, dir_out)

func get_dir_in() -> String:
    return self.dir_in

func get_dir_out() -> String:
    return self.dir_out

## Return the current file being scanned/processed, or null if there is no scan in progress.
func get_current_file() -> File:
    return self.current_file

func is_scan_in_progress() -> bool:
    return self._is_scan_in_progress

func set_dir_in(dir_in: String) -> void:
    if dir_in and not _path_is_subpath(dir_in, dir_out):
        self.dir_in = dir_in
        self.dir_in_files = _get_files_recursive(dir_in)
        print("FO: dir_in set to: %s" % dir_in)
    else:
        print("FO: invalid dir_in: %s" % dir_in)

func set_dir_out(dir_out: String) -> void:
    if dir_out and not _path_is_subpath(dir_in, dir_out):
        self.dir_out = dir_out
        print("FO: dir_out set to: %s" % dir_out)
    else:
        print("FO: invalid dir_out: %s" % dir_out)

func queue_stop_scan() -> void:
    _stop_requested = true

func start_scan(process := false) -> void:
    if _scan_thread.is_alive():
        _stop_requested = true
        _scan_thread.wait_to_finish()
    
    _scan_thread = Thread.new()
    _stop_requested = false
    print("FO: starting scan of %s..." % dir_in)
    _is_scan_in_progress = true
    scan_started.emit()
    _scan_thread.start(_start_scan.bind(process))


func _start_scan(process := false) -> void:
    if !dir_in: return

    print("starting scan:")
    for prop: StringName in [
        "converter_other_formats_enabled",
        "converter_other_formats",
        "converter_bit_depth_enabled",
        "converter_target_bit_depth",
        "converter_sample_rate_enabled",
        "converter_target_sample_rate",
        "converter_auto_mono_enabled",
        "converter_auto_mono_threshold",

        "renamer_remove_common_prefixes_enabled",
        "renamer_remove_common_suffixes_enabled",
        "renamer_minimize_path_enabled",
        "renamer_minimize_path_keep_numbers",
        "renamer_uppercase_enabled",
        "renamer_replace_spaces_enabled",
        "renamer_replace_spaces_char",
    ]:
        print("    %s = %s" % [prop, get(prop)])

    # reset file counters
    num_files_total = dir_in_files.size()
    num_files_scanned = 0
    num_files_copied = 0
    num_files_converted = 0
    num_files_skipped = 0

    paths_out.clear()

    for full_path_in: String in dir_in_files:
        if _stop_requested:
            print("scanning stopped early")
            _is_scan_in_progress = false
            scan_finished.emit.call_deferred()
            return

        # path starting from source directory
        var path_in := full_path_in.trim_prefix("%s" % dir_in).trim_prefix("/")

        # convert bit depth operation
        current_file = File.new(path_in, self)
        current_file.prepare_rename()

        var _skip_pattern_fn := func(pattern: String) -> bool:
            if pattern.begins_with("!"):
                return not path_in.match(pattern.substr(1))
            else:
                return path_in.match(pattern)

        var skip_pattern_is_match := skip_patterns.all(_skip_pattern_fn) if skip_pattern_and else skip_patterns.any(_skip_pattern_fn)

        var do_skip := (
            skip_pattern_is_match
            or path_in.get_extension() == ""
            or FileAccess.file_exists("%s/%s" % [dir_out, current_file.path_out])
        )

        # skipped (do not copy) conditions
        if do_skip:
            item_skipped_queue.append(current_file)
            num_files_skipped += 1
            print("skipped: %s" % path_in)

        var do_ignore := ignore_patterns.any(func(pattern: String) -> bool:
            return path_in.match(pattern)
        )

        # ignored (copy unmodified) conditions
        if !do_skip and do_ignore:
            item_copied_queue.append(current_file)
            num_files_copied += 1
            print("copied (ignored): %s" % path_in)


        elif !do_skip and !do_ignore:
            current_file.prepare_conversion()
            # converted (copy modified) conditions
            if current_file.wav_invalid:
                _mutex.lock()
                item_skipped_queue.append(current_file)
                _mutex.unlock()
                num_files_skipped += 1
                print("skipped: %s" % path_in)
            elif current_file.is_conversion_needed() or current_file.is_rename_needed():
                _mutex.lock()
                item_converted_queue.append(current_file)
                _mutex.unlock()
                num_files_converted += 1
                print("converted/renamed: %s" % path_in)
            else:
                _mutex.lock()
                item_copied_queue.append(current_file)
                _mutex.unlock()
                num_files_copied += 1
                print("copied: %s" % path_in)

        if process and !do_skip: current_file.process()
        paths_out.append(current_file.path_out)

        num_files_scanned += 1

    current_file = null
    _is_scan_in_progress = false
    scan_finished.emit.call_deferred()

##
## Return true if path [a] is inside path [b] or vice versa.
##
func _path_is_subpath(a: String, b: String) -> bool:
    a = a + "/"
    b = b + "/"
    return a.begins_with(b) or b.begins_with(a)

##
## Check if [text] matches any of the patterns in [patterns].
## Accepts "*" for globs. If a pattern starts with "!", invert the check.
##
func _check_pattern_match(text: String, patterns: Array[String]) -> bool:
    return patterns.any(func(pattern: String) -> bool:
        if pattern.begins_with("!"):
            return not text.match(pattern.substr(1))
        else:
            return text.match(pattern)
    )

##
## Recursively get all file paths (absolute) in a directory.
##
func _get_files_recursive(dirpath: String) -> PackedStringArray:
    var dir := DirAccess.open(dirpath)
    dir.include_hidden = true

    var filepaths: PackedStringArray = []

    # get all .wavs in folder
    for filename in dir.get_files():
        filepaths.append(dir.get_current_dir(true) + "/" + filename)

    for dirname in dir.get_directories():
        filepaths.append_array(_get_files_recursive(dir.get_current_dir(true) + "/" + dirname))
    
    return filepaths
