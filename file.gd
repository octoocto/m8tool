class_name File

const MAX_PATH_LENGTH := 127

var sm: FileOptimizer

var is_scanned := false
var full_path_in: String
var full_path_out: String
var path_in: String
var path_out: String

var wav_bit_depth: int
var wav_sample_rate: int
var wav_channels: int
var wav_stereo_diff: float # average volume (dB) after inverting stereo phase
var wav_invalid: bool

var regex := RegEx.new()

func _init(path_in: String, sm: FileOptimizer) -> void:
    self.sm = sm
    self.path_in = path_in
    self.full_path_in = "%s/%s" % [sm.dir_in, path_in]

    self.path_out = path_in
    self.full_path_out = "%s/%s" % [sm.dir_out, path_out]

func is_conversion_needed() -> bool:
    if is_valid_audio_format():
        # check bit depth
        if sm.converter_bit_depth_enabled and wav_bit_depth > sm.converter_target_bit_depth:
            return true

        # check sample rate
        if sm.converter_sample_rate_enabled and wav_sample_rate > sm.converter_target_sample_rate:
            return true

        # check auto mono
        if sm.converter_auto_mono_enabled and wav_stereo_diff <= sm.converter_auto_mono_threshold:
            return true

    return false

func is_rename_needed() -> bool:
    return path_in != path_out

func is_valid_audio_format() -> bool:
    if sm.converter_other_formats_enabled:
        return path_in.get_extension().to_lower() == "wav" or sm.converter_other_formats.has(path_in.get_extension().to_lower())
    else:
        return path_in.get_extension().to_lower() == "wav"

##
## Prepare to rename this file.
##
func prepare_rename() -> void:
    self.path_out = _generate_path_out()
    self.full_path_out = "%s/%s" % [sm.dir_out, path_out]

##
## Prepare to convert this file (if this file is a WAV).
## This will run "ffprobe".
##
func prepare_conversion() -> void:
    if is_valid_audio_format():
        var ffprobe_text := _ffprobe()

        wav_bit_depth = _regex_match(ffprobe_text, "(?<=bits_per_sample=)(\\d+)").to_int()
        wav_sample_rate = _regex_match(ffprobe_text, "(?<=sample_rate=)(\\d+)").to_int()
        wav_channels = _regex_match(ffprobe_text, "(?<=channels=)(\\d+)").to_int()

        if wav_channels == 2:
            wav_stereo_diff = _ffmpeg_get_stereo_difference()

        # ensure output path is .wav
        self.path_out = "%s.%s" % [self.path_out.get_basename(), "WAV" if sm.renamer_uppercase_enabled else "wav"]
        self.full_path_out = "%s.%s" % [self.full_path_out.get_basename(), "WAV" if sm.renamer_uppercase_enabled else "wav"]

    is_scanned = true

##
## Process this file.
##
## If [prepare_conversion()] was called before this method,
## "ffmpeg" will be run to convert this file.
## Otherwise, the file is copied without any conversion.
##
## If [prepare_rename()] was called before this method,
## the output filepath may be different, otherwise it will be
## the same as the input filepath.
##
func process() -> void:
    if is_conversion_needed():
        # set up ffmpeg flags
        var args: Array[String] = ["ffmpeg", "-y", "-i", full_path_in]

        # check bit depth
        if sm.converter_bit_depth_enabled and wav_bit_depth > sm.converter_target_bit_depth:
            args.append_array(["-acodec", "pcm_s%dle" % sm.converter_target_bit_depth])

        # check sample rate
        if sm.converter_sample_rate_enabled and wav_sample_rate > sm.converter_target_sample_rate:
            args.append_array(["-ar", "%d" % sm.converter_target_sample_rate])

        # check sample rate
        if sm.converter_auto_mono_enabled and wav_stereo_diff <= sm.converter_auto_mono_threshold:
            args.append_array(["-ac", "1"])

        args.append(full_path_out)

        DirAccess.open(sm.dir_out).make_dir_recursive(path_out.get_base_dir())

        var res := sm.shell.run(args)
        if res.return_code != 0:
            printerr(res.stdout)

    # just copy the file
    else:
        var dir := DirAccess.open(sm.dir_out)
        if !dir.dir_exists(path_out.get_base_dir()):
            dir.make_dir_recursive(path_out.get_base_dir())
        dir.copy(full_path_in, full_path_out)


##
## Generates a new output filepath for this file using the
## current renamer rules.
##
func _generate_path_out() -> String:
    var out_file_path := ""
    var iterations := 0

    while out_file_path == "" or sm.paths_out.has(out_file_path):
        var out_extension := path_in.get_extension()
        var out_basename := path_in.get_basename()

        # remove any common prefixes
        if sm.renamer_remove_common_prefixes_enabled:
            var prefix := _get_common_prefix()
            # print("file.gd: %s: detected prefix = %s" % [out_basename.get_file(), prefix])
            var file_name := out_basename.get_file().trim_prefix(prefix)
            out_basename = "%s/%s" % [out_basename.get_base_dir(), file_name]
        
        # remove any common suffixes
        if sm.renamer_remove_common_suffixes_enabled:
            var suffix := _get_common_suffix()
            # print("file.gd: detected suffix = %s" % suffix)
            var file_name := out_basename.get_file().trim_suffix(suffix)
            out_basename = "%s/%s" % [out_basename.get_base_dir(), file_name]

        if out_basename == "": out_basename = "0"

        # minimize
        if sm.renamer_minimize_path_enabled:
            out_basename = _minimize_path(out_basename)

        # prevent duplicate names
        if iterations > 0:
            out_basename = out_file_path.get_basename()
            if out_basename[-1].is_valid_int():
                out_basename = out_basename.left(len(out_basename) - 1) + "%d" % (out_basename[-1].to_int() + 1)
            else:
                out_basename = out_basename + " %d" % (iterations - 1)
            push_warning("%s exists, renaming to %s" % [out_file_path.get_basename(), out_basename])

        # replace spaces
        if sm.renamer_minimize_path_enabled and sm.renamer_replace_spaces_enabled:
            out_basename = out_basename.replace(" ", sm.renamer_replace_spaces_char)

        if out_extension:
            out_file_path = "%s.%s" % [out_basename, out_extension]
        else:
            out_file_path = out_basename

        # uppercase
        if sm.renamer_uppercase_enabled:
            out_file_path = out_file_path.to_upper()

        # truncate
        # if output_file_path.length() > MAX_PATH_LENGTH:
        #     output_file_path = output_file_path.substr(0, MAX_PATH_LENGTH)

        iterations += 1
        
    # print("file.gd: %s -> %s" % [path_in, out_file_path])

    return out_file_path

##
## Minimize a path name. This will:
## - Remove any common punctuation characters
## - Remove any duplicate words in the path
##
func _minimize_path(path: String) -> String:
    var unique_words := {}
    var basedir := path.get_base_dir()
    var filename := path.get_file()

    for c in sm.renamer_split_chars: basedir = basedir.replace(c, " ")
    for c in sm.renamer_fill_chars: basedir = basedir.replace(c, "")

    # minimize all dirnames separately
    var dirnames := Array(basedir.split("/")).map(func(part: String) -> String:
        return _minimize_name(part, unique_words)
    ).filter(func(part: String) -> bool:
        return part != ""
    )

    # minimize filename
    if (sm.renamer_minimize_path_mode == 1 and is_valid_audio_format()) or sm.renamer_minimize_path_mode == 2:
        for c in sm.renamer_split_chars: filename = filename.replace(c, " ")
        for c in sm.renamer_fill_chars: filename = filename.replace(c, "")
        filename = _minimize_name(filename, unique_words)

    return ("%s/%s" % ["/".join(dirnames), filename]).trim_prefix("/")

func _minimize_name(name: String, unique_words: Dictionary) -> String:
    var words := Array(name.split(" ", false))

    # remove words found in unique_words
    words = words.filter(func(word: String) -> bool:
        if word.is_valid_int() and sm.renamer_minimize_path_keep_numbers:
            return true
        return !unique_words.has(word.to_lower())
    )

    # remove prefixes found in blacklist
    words = words.filter(func(word: String) -> bool:
        return !sm.renamer_prefix_blacklist.any(func(prefix: String) -> bool:
            return word.to_lower().begins_with(prefix)
        )
    )

    # add words to unique_words
    for word: String in words:
        unique_words[word.to_lower()] = null
        # also add plural or singular form of every word
        if word.to_lower().ends_with("s"):
            unique_words[word.to_lower().substr(0, word.length() - 1)] = null
        else:
            unique_words[word.to_lower() + "s"] = null

    return " ".join(words)

func _get_common_prefix() -> String:
    # filter all files to only ones in the same directory as this one (ignore extension)
    var files := Array(sm.dir_in_files).filter(func(path: String) -> bool:
        return path.begins_with("%s/%s" % [sm.dir_in, path_in.get_base_dir()])
    )
    if len(files) <= 1: return ""
    files = files.map(func(file: String) -> String: return file.get_file().get_basename())

    var test_str: String = files[0]
    var length: int = len(test_str)

    var prefix := ""
    for i in length:
        prefix += test_str[i]
        if !files.all(func(file: String) -> bool: return file.to_lower().begins_with(prefix.to_lower())):
            if i == length: return ""
            return test_str.substr(0, i)

    return "" # prevent prefix being the entire filename

func _get_common_suffix() -> String:
    var files := Array(sm.dir_in_files).filter(func(path: String) -> bool:
        return path.begins_with("%s/%s" % [sm.dir_in, path_in.get_base_dir()])
    )
    if files.size() <= 1: return ""
    files = files.map(func(file: String) -> String: return file.get_file().get_basename())

    var test_str: String = files[0]
    var length: int = len(test_str)

    var suffix := ""
    for i in length:
        suffix = test_str.substr(length - 1 - i, length - 1)
        if !files.all(func(file: String) -> bool:
            return file.to_lower().ends_with(suffix.to_lower())
        ):
            if i == length: return ""
            return test_str.substr(length - i, length - 1)
    return "" # prevent suffix being the entire filename

func _get_user_dir() -> String:
    var dir := DirAccess.open("user://")
    return ProjectSettings.globalize_path(dir.get_current_dir())

func _regex_match(text: String, pattern: String) -> String:
    regex.compile(pattern)
    var result := regex.search(text)
    if result:
        return result.get_string()
    else:
        return ""

func _remove_unsupported_cha(text: String, pattern: String, with: String) -> String:
    regex.compile(pattern)
    return regex.sub(text, with, true)

func _ffprobe() -> String:
    var res := sm.shell.run(["ffprobe", "-i", "%s/%s" % [sm.dir_in, path_in], "-show_streams", "-select_streams", "a:0"])
    if res.return_code != 0:
        print(res.command)
        print(res.stdout)
        wav_invalid = true
    return res.stdout

func _ffmpeg_get_stereo_difference() -> float:
    return sm.shell.ffprobe_get_stereo_difference("%s/%s" % [sm.dir_in, path_in])
