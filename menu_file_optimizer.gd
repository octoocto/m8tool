@tool
class_name MenuFileOptimizer extends VBoxContainer

const ICON_CONVERTED := preload("res://assets/StatusConverted.png")
const ICON_COPIED := preload("res://assets/StatusSuccess.png")
const ICON_SKIPPED := preload("res://assets/StatusSkip.png")

enum ProcessMethod {
    PROCESS_TO_DEST,
    PROCESS_IN_PLACE,
}

enum ProcessedType {
    CONVERTED, COPIED, SKIPPED
}

enum TreeFilter {
    ALL, CONVERTED, COPIED, SKIPPED
}

const STATUS_BAR_TEXT := "[p tab_stops=80,80,80]📁 %d	🟢 %d	🟠 %d	⚪ %d[/p]"


var shell: Shell = preload("res://shell.gd").new()

var tree_filter := TreeFilter.ALL

var time_started := 0
var time_ended := 0

var fo := FileOptimizer.new()

@onready var tree: Tree = %Tree
@onready var progress_bar: ProgressBar = %ProgressBar
@onready var status_bar: RichTextLabel = %StatusBar

func _ready() -> void:
    if Engine.is_editor_hint(): return

    # tree setup

    tree.create_item()

    tree.set_column_clip_content(0, false)
    tree.set_column_title_alignment(0, HORIZONTAL_ALIGNMENT_LEFT)
    tree.set_column_expand(0, true)
    tree.set_column_clip_content(0, true)

    tree.set_column_title_alignment(1, HORIZONTAL_ALIGNMENT_RIGHT)
    tree.set_column_custom_minimum_width(1, 60)
    tree.set_column_expand(1, false)

    tree.set_column_title_alignment(2, HORIZONTAL_ALIGNMENT_LEFT)
    tree.set_column_expand(2, false)
    tree.set_column_custom_minimum_width(2, 70)
    tree.set_column_clip_content(2, false)

    tree.set_column_title_alignment(3, HORIZONTAL_ALIGNMENT_RIGHT)
    tree.set_column_expand(3, false)
    tree.set_column_custom_minimum_width(3, 60)
    tree.set_column_clip_content(3, false)

    tree.gui_input.connect(func(event: InputEvent) -> void:
        if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_RIGHT:
            tree.accept_event()
            var item := tree.get_item_at_position(event.position)
            tree.set_selected(item, 0)
            DisplayServer.clipboard_set(item.get_tooltip_text(0))
            print("copied tooltip")
    )

    # input/output folder controls

    %ButtonOpenSrcPath.pressed.connect(func() -> void:
        %FileDialogSource.show()
    )

    %ButtonOpenDestPath.pressed.connect(func() -> void:
        %FileDialogDest.show()
    )

    %FileDialogSource.dir_selected.connect(func(dir: String) -> void:
        fo.set_dir_in(dir)
        _update_buttons()
    )

    %FileDialogDest.dir_selected.connect(func(dir: String) -> void:
        fo.set_dir_out(dir)
        _update_buttons()
    )

    %ButtonScan.pressed.connect(func() -> void:
        if fo.is_dir_in_valid():
            if fo.is_scan_in_progress():
                fo.queue_stop_scan()
            else:
                start_scan_source_dir(false)
    )

    %ButtonProcess.pressed.connect(func() -> void:
        if fo.is_dir_in_and_out_valid():
            if fo.is_scan_in_progress():
                fo.queue_stop_scan()
            else:
                start_scan_source_dir(true)
    )

    fo.scan_started.connect(_update_buttons)
    fo.scan_finished.connect(_update_buttons)

    # setting controls

    %SettingConvertFormats.init(null, func(value: bool) -> void:
        fo.converter_other_formats_enabled = value
    )
    %LineEditConvertFormats.text_changed.connect(func(text: String) -> void:
        fo.converter_other_formats = text.to_lower().replace(".", "").split(",")
        print("set other formats to convert = %s" % [fo.converter_other_formats])
    )
    %LineEditConvertFormats.text_changed.emit(%LineEditConvertFormats.text)
    %SettingConvertBitDepth.init(null, func(value: bool) -> void:
        fo.converter_bit_depth_enabled = value
    )
    %OptionConvertBitDepth.item_selected.connect(func(index: int) -> void:
        match index:
            0:
                fo.converter_target_bit_depth = 8
            1:
                fo.converter_target_bit_depth = 16
            2:
                fo.converter_target_bit_depth = 24
            3:
                fo.converter_target_bit_depth = 32
        print("set target bit depth = %d" % fo.converter_target_bit_depth)
    )
    %SettingConvertSampleRate.init(null, func(value: bool) -> void:
        fo.converter_sample_rate_enabled = value
    )
    %OptionConvertSampleRate.item_selected.connect(func(index: int) -> void:
        match index:
            0:
                fo.converter_target_sample_rate = 44100
            1:
                fo.converter_target_sample_rate = 48000
        print("set target sample rate = %d" % fo.converter_target_sample_rate)
    )
    %SettingConvertMono.init(null, func(value: bool) -> void:
        fo.converter_auto_mono_enabled = value
    )

    %SettingMinimizePath.init(null, func(value: bool) -> void:
        fo.renamer_minimize_path_enabled = value
    )
    %SettingMinimizePathSep.init(null, func(value: bool) -> void:
        fo.renamer_replace_spaces_enabled = value
    )
    %LineEditMinimizePathSep.text_changed.connect(func(text: String) -> void:
        if len(text) > 1:
            fo.renamer_replace_spaces_char = text.substr(0, 1)
        else:
            fo.renamer_replace_spaces_char = text
    )
    %SettingMinimizePathKeepNumbers.init(null, func(value: bool) -> void:
        fo.renamer_minimize_path_keep_numbers = value
    )
    # %SettingPathCleanName.init(null, func(value: bool) -> void:
    #     fo.renamer_replace_spaces_enabled = value
    # )
    %SettingPathRemovePrefixes.init(null, func(value: bool) -> void:
        fo.renamer_remove_common_prefixes_enabled = value
    )
    %SettingPathRemoveSuffixes.init(null, func(value: bool) -> void:
        fo.renamer_remove_common_suffixes_enabled = value
    )
    %SettingPathUppercase.init(null, func(value: bool) -> void:
        fo.renamer_uppercase_enabled = value
    )

    %LineEditIgnorePattern.text_changed.connect(func(text: String) -> void:
        fo.ignore_patterns = Array(text.split(","))
        print("set ignore patterns = %s" % [fo.ignore_patterns])
    )
    %LineEditIgnorePattern.text_changed.emit(%LineEditIgnorePattern.text)

    %LineEditRemovePattern.text_changed.connect(func(text: String) -> void:
        fo.skip_patterns = Array(text.split(","))
        print("set skip patterns = %s" % [fo.skip_patterns])
    )
    %LineEditRemovePattern.text_changed.emit(%LineEditRemovePattern.text)
    %CheckButtonSkipPatternAnd.toggled.connect(func(value: bool) -> void:
        fo.skip_pattern_and = value
        print("set skip pattern AND = %s" % [value])
    )


    # status bar controls

    %ButtonFilesTotal.pressed.connect(func() -> void:
        %ButtonFilesCopied.set_pressed_no_signal(false)
        %ButtonFilesSkipped.set_pressed_no_signal(false)
        %ButtonFilesConverted.set_pressed_no_signal(false)
        tree_filter = TreeFilter.ALL
    )
    %ButtonFilesConverted.toggled.connect(func(value: bool) -> void:
        if value:
            %ButtonFilesCopied.set_pressed_no_signal(false)
            %ButtonFilesSkipped.set_pressed_no_signal(false)
            tree_filter = TreeFilter.CONVERTED
        else:
            tree_filter = TreeFilter.ALL
    )
    %ButtonFilesCopied.toggled.connect(func(value: bool) -> void:
        if value:
            %ButtonFilesConverted.set_pressed_no_signal(false)
            %ButtonFilesSkipped.set_pressed_no_signal(false)
            tree_filter = TreeFilter.COPIED
        else:
            tree_filter = TreeFilter.ALL
    )
    %ButtonFilesSkipped.toggled.connect(func(value: bool) -> void:
        if value:
            %ButtonFilesConverted.set_pressed_no_signal(false)
            %ButtonFilesCopied.set_pressed_no_signal(false)
            tree_filter = TreeFilter.SKIPPED
        else:
            tree_filter = TreeFilter.ALL
    )

func _update_buttons() -> void:
    %LineEditSrcPath.text = fo.get_dir_in()
    %LineEditDestPath.text = fo.get_dir_out()
    if fo.is_dir_in_and_out_valid():
        %ButtonScan.disabled = false
        %ButtonProcess.disabled = false
    elif fo.is_dir_in_valid():
        %ButtonScan.disabled = false
        %ButtonProcess.disabled = true
    else:
        %ButtonScan.disabled = true
        %ButtonProcess.disabled = true

    if fo.is_scan_in_progress():
        %ButtonScan.text = "Cancel"
        %ButtonProcess.text = "Cancel"
    else:
        %ButtonScan.text = "Scan Files"
        %ButtonProcess.text = "Process Files"

## Return true if path [a] is inside path [b] or vice versa.
func _path_is_subpath(a: String, b: String) -> bool:
    a = a + "/"
    b = b + "/"
    return a.begins_with(b) or b.begins_with(a)

func _physics_process(_delta: float) -> void:
    if Engine.is_editor_hint(): return

    if fo.is_scan_in_progress():
        time_ended = Time.get_ticks_msec()

    for item in fo.item_copied_queue:
        tree.get_root().add_child(_create_file_item(item, ProcessedType.COPIED))
    fo.item_copied_queue = []

    for item in fo.item_converted_queue:
        tree.get_root().add_child(_create_file_item(item, ProcessedType.CONVERTED))
    fo.item_converted_queue = []

    for item in fo.item_skipped_queue:
        tree.get_root().add_child(_create_file_item(item, ProcessedType.SKIPPED))
    fo.item_skipped_queue = []

    # apply tree filters
    for item in tree.get_root().get_children():
        match tree_filter:
            TreeFilter.ALL:
                item.visible = true
            TreeFilter.CONVERTED:
                item.visible = item.get_metadata(0) == ProcessedType.CONVERTED
            TreeFilter.COPIED:
                item.visible = item.get_metadata(0) == ProcessedType.COPIED
            TreeFilter.SKIPPED:
                item.visible = item.get_metadata(0) == ProcessedType.SKIPPED

    # status bar text
    %ButtonFilesTotal.text = "%d" % fo.num_files_total
    %ButtonFilesTotal.tooltip_text = "Total files: %d" % fo.num_files_total
    %ButtonFilesConverted.text = "%d" % fo.num_files_converted
    %ButtonFilesConverted.tooltip_text = "Converted/renamed files: %d" % fo.num_files_converted
    %ButtonFilesCopied.text = "%d" % fo.num_files_copied
    %ButtonFilesCopied.tooltip_text = "Copied (unmodified) files: %d" % fo.num_files_copied
    %ButtonFilesSkipped.text = "%d" % fo.num_files_skipped
    %ButtonFilesSkipped.tooltip_text = "Skipped files: %d" % fo.num_files_skipped

    # progress bar value/text
    progress_bar.value = fo.get_scan_progress()
    if time_started == 0:
        %LabelProgress.text = "%.1f%%" % progress_bar.value
    else:
        %LabelProgress.text = "%s %.1f%%" % [_get_time_string(), progress_bar.value]
    %LabelFile.text = fo.get_current_file().path_in if fo.get_current_file() else ""

func start_scan_source_dir(process := false) -> void:
    tree.clear()
    tree.create_item()
    # item_copied = tree.create_item()
    # item_converted = tree.create_item()
    # item_skipped = tree.create_item()

    time_started = Time.get_ticks_msec()
    fo.start_scan(process)


func _get_time_string() -> String:
    var time := time_ended - time_started

    var h := time / 1000 / 60 / 60
    var m := time / 1000 / 60 % 60
    var s := time / 1000 % 60

    return "%02d:%02d:%02d" % [h, m, s]

##
## Recursively get all file paths (absolute) in a directory.
##
func get_files_recursive(dirpath: String) -> PackedStringArray:
    var dir := DirAccess.open(dirpath)

    var filepaths: PackedStringArray = []

    # get all .wavs in folder
    for filename in dir.get_files():
        filepaths.append(dir.get_current_dir(true) + "/" + filename)

    for dirname in dir.get_directories():
        filepaths.append_array(get_files_recursive(dir.get_current_dir(true) + "/" + dirname))
    
    return filepaths

func _create_file_item(file: File, type: ProcessedType) -> TreeItem:
    var item := tree.create_item(null, -1)

    item.set_text(0, file.path_in)
    item.set_tooltip_text(0, file.path_in)
    item.set_metadata(0, type)

    match type:
        ProcessedType.CONVERTED:
            item.set_icon(0, ICON_CONVERTED)
        ProcessedType.COPIED:
            item.set_icon(0, ICON_COPIED)
        ProcessedType.SKIPPED:
            item.set_icon(0, ICON_SKIPPED)

    if file.is_scanned:
        # renamer
        if file.is_rename_needed():
            var child := tree.create_item(item, -1)
            child.set_selectable(0, false)
            child.set_text(0, "Rename to \"%s\" (len: %d -> %d)" % [file.path_out, len(file.path_in), len(file.path_out)])

            if file.is_valid_audio_format() and file.path_out.length() > 127:
                item.set_custom_color(0, Color.YELLOW)
                child = tree.create_item(item, -1)
                child.set_selectable(0, false)
                child.set_text(0, "Note: path is still over 127 chars and still won't be read by the M8.")

        if file.is_valid_audio_format():
            # channels
            item.set_text(1, "mono" if file.wav_channels == 1 else "stereo")
            if file.wav_stereo_diff <= fo.converter_auto_mono_threshold:
                item.set_custom_color(1, Color.YELLOW)
                var child := tree.create_item(item, -1)
                child.set_selectable(0, false)
                child.set_text(0, "Convert audio to mono. Average volume after inverting 2nd channel = %.2f dB" % file.wav_stereo_diff)
            # bit rate
            item.set_text(2, "%d Hz" % file.wav_sample_rate)
            if file.wav_sample_rate > fo.converter_target_sample_rate:
                item.set_custom_color(2, Color.YELLOW)
                var child := tree.create_item(item, -1)
                child.set_selectable(0, false)
                child.set_text(0, "Convert sample rate to %d Hz" % fo.converter_target_sample_rate)
            # bit depth
            item.set_text(3, "%d bit" % file.wav_bit_depth)
            if file.wav_bit_depth > fo.converter_target_bit_depth:
                item.set_custom_color(3, Color.YELLOW)
                var child := tree.create_item(item, -1)
                child.set_selectable(0, false)
                child.set_text(0, "Convert bit depth to to %d-bit" % fo.converter_target_bit_depth)

        item.collapsed = true

    tree.get_root().remove_child(item)
    return item
