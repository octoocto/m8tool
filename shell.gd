class_name Shell


var _version_ffmpeg: String
var _version_ffprobe: String
var _version_python: String
var _version_py_colorthief: String


##
## Check versions of required apps.
##
func check_versions() -> void:
    _version_ffmpeg = get_ffmpeg_version()
    _version_ffprobe = get_ffprobe_version()
    _version_python = get_python_version()
    if python_run("import importlib.util; print(importlib.util.find_spec('colorthief') != None)") == "True":
        _version_py_colorthief = python_run("import colorthief; print(colorthief.__version__)")
    else:
        _version_py_colorthief = ""


func regex_match(pattern: String, text: String) -> String:
    var re := RegEx.new()
    re.compile(pattern)
    var result := re.search(text)
    if result:
        return result.get_string()
    else:
        return ""
        
func run(args: Array[String]) -> Response:
    print("$ %s" % " ".join(args.map(func(arg: String) -> String:
        return "\"%s\"" % arg if arg.contains(" ") else arg
    )))

    var path: String = args.pop_front()
    var output := []
    var return_code := OS.execute(path, args, output, true, false)

    var res := Response.new()
    res.command = "%s %s" % [path, " ".join(args)]
    res.return_code = return_code
    res.stdout = output[0]
    if output.size() == 2:
        res.stderr = output[1]
        printerr(res.stderr)
    else:
        res.stderr = ""

    # command_executed.emit(res)
    return res

##
## Check the installed ffmpeg version.
## If ffmpeg is not installed, an empty string is returned.
## 
func get_ffmpeg_version() -> String:
    if not _version_ffmpeg:
        var res := run(["ffmpeg", "-version"])
        _version_ffmpeg = regex_match("(?<=ffmpeg version )([\\w\\.-]+)", res.stdout)

    return _version_ffmpeg

func get_ffprobe_version() -> String:
    if not _version_ffprobe:
        var res := run(["ffprobe", "-version"])
        _version_ffprobe = regex_match("(?<=ffprobe version )([\\w\\.-]+)", res.stdout)

    return _version_ffprobe

func get_python_version() -> String:
    if not _version_python:
        var res := run(["python", "--version"])
        _version_python = regex_match("(?<=Python )([\\w\\.-]+)", res.stdout)

    return _version_python

func python_run(script: String) -> String:
    var res := run(["python", "-c", script])
    return res.stdout.strip_edges()

func get_python_module_version(python_module: String) -> String:
    if get_python_version() == "":
        return ""

    if python_run("import importlib.util; print(importlib.util.find_spec('{0}') != None)".format([python_module])) == "True":
        return python_run("import {0}; print({0}.__version__)".format([python_module]))
    else:
        return ""

func ffprobe_get_bits_per_sample(sample_path: String) -> int:
    var res := run(["ffprobe", "-show_streams", sample_path])
    var bit_depth := regex_match("(?<=bits_per_sample=)(\\d+)", res.stdout).to_int()
    return bit_depth

func ffprobe_get_bit_rate(sample_path: String) -> int:
    var res := run(["ffprobe", "-show_streams", sample_path])
    var bitrate := regex_match("(?<=bit_rate=)(\\d+)", res.stdout).to_int()
    return bitrate

func ffprobe_get_channels(sample_path: String) -> int:
    var res := run(["ffprobe", "-show_streams", sample_path])
    var bitrate := regex_match("(?<=channels=)(\\d+)", res.stdout).to_int()
    return bitrate

func ffprobe_get_stereo_difference(sample_path: String) -> float:
    var res := run([
        "ffmpeg",
        "-i", sample_path,
        "-filter_complex", "stereotools=phasel=1[tmp];[tmp]pan=1c|c0=0.5*c0+0.5*c1,volumedetect",
        "-f", "null", "/dev/null"
    ])
    return regex_match("(?<=mean_volume: )([-\\d.]+)", res.stdout).to_float()


##
## A response from a shell execution.
##
class Response:
    ## The full command that was executed
    var command: String
    ## The response code of the executed command
    var return_code: int
    ## The text of the command that was printed to stdout
    var stdout: String
    ## The text of the command that was printed to stderr
    var stderr: String
