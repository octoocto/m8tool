@tool
extends PanelContainer

const TEX_OK := preload("res://assets/StatusSuccess.png")
const TEX_WARNING := preload("res://assets/StatusWarning.png")
const TEX_ERROR := preload("res://assets/StatusError.png")

var shell := Shell.new()

@onready var tabs: TabContainer = %TabContainer

func _ready() -> void:
    %ExitButton.pressed.connect(func() -> void: get_tree().quit())

    _check_versions()

    # set up home buttons
    %ButtonFileOptimizer.pressed.connect(func() -> void:
        %HomeContainer.hide()
        %"File Optimizer".show()
    )
    %ButtonThemeGenerator.pressed.connect(func() -> void:
        %HomeContainer.hide()
        %"Theme Generator".show()
    )
    %ButtonUtilities.pressed.connect(func() -> void:
        %HomeContainer.hide()
        %Utilities.show()
    )

    # set up tempo/swing calculation
    %RangeDesiredTempo.value_changed.connect(func(_value: float) -> void: _calculate_tempo())
    %RangeDesiredTickRes.value_changed.connect(func(_value: float) -> void: _calculate_tempo())
    %RangeDesiredSwing.value_changed.connect(func(_value: float) -> void: _calculate_tempo())
    _calculate_tempo()

func _check_versions() -> void:
    var vers_ffmpeg := shell.get_ffmpeg_version()
    var vers_ffprobe := shell.get_ffprobe_version()
    var vers_python := shell.get_python_version()
    var vers_py_colorthief := shell.get_python_module_version("colorthief")

    %LineFFmpeg.text = "not found" if vers_ffmpeg == "" else vers_ffmpeg
    %LineFFprobe.text = "not found" if vers_ffprobe == "" else vers_ffprobe
    %LinePython.text = "not found" if vers_python == "" else vers_python
    %LinePythonColorthief.text = "not found" if vers_py_colorthief == "" else vers_py_colorthief

    %TextureRectFFmpeg.texture = TEX_ERROR if vers_ffmpeg == "" else TEX_OK
    %TextureRectFFprobe.texture = TEX_ERROR if vers_ffprobe == "" else TEX_OK
    %TextureRectPython.texture = TEX_ERROR if vers_python == "" else TEX_OK
    %TextureRectPythonColorthief.texture = TEX_ERROR if vers_py_colorthief == "" else TEX_OK

    if not vers_ffmpeg or not vers_ffprobe:
        # %LabelFFmpeg.text = "[color=yellow]ffmpeg and/or ffprobe not found. converting samples will not be available."
        %WarningLabel.text += "[ul][color=yellow]FFmpeg and/or FFprobe not found. Converting samples will not be available.[/color][/ul]\n"
        %ButtonFileOptimizer.disabled = true

    # if not vers_python:
    #     %LabelPython.text = "[color=red]python not found."

    # if not vers_py_colorthief:
    #     %LabelPythonColorthief.text = "[color=yellow]colorthief not found. theme generation from images will not be available."

    if not vers_python or not vers_py_colorthief:
        %WarningLabel.text += "[ul][color=yellow]Python and/or colorthief package not found. Generating themes will not be available.[/color][/ul]\n"
        %ButtonThemeGenerator.disabled = true


func _calculate_tempo() -> void:
    var desired_tempo: float = %RangeDesiredTempo.value
    var desired_ticks: int = %RangeDesiredTickRes.value
    var desired_swing: float = %RangeDesiredSwing.value / 100.0

    var project_tempo: float = desired_ticks / 6.0 * desired_tempo

    var project_tick_1: int = floor(float(desired_ticks * 2) * desired_swing)
    var project_tick_2: int = (desired_ticks * 2) - project_tick_1
    var project_groove: String = "%d, %d" % [project_tick_1, project_tick_2]
    var project_swing: float = project_tick_1 / float(project_tick_1 + project_tick_2)
    var project_ms_per_tick: float = (1.0 / desired_tempo) * 60.0 / 4.0 / float(desired_ticks) * 1000.0

    %RangeProjectTempo.value = project_tempo
    %LabelProjectGroove.text = project_groove
    %RangeProjectSwing.value = project_swing * 100
    %RangeProjectMSPerTick.value = project_ms_per_tick