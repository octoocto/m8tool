extends PanelContainer

const ICON_FOLDER := preload("res://icon/folder.svg")
const ICON_DRIVE := preload("res://icon/device-ipad.svg")

const CONFIG_PATH := "user://config.ini"

enum SourcePathMode {
	DRIVE,
	DIRECTORY,
}

@onready var button_close: Button = %ButtonClose
@onready var button_start: Button = %ButtonStart
@onready var button_dry_start: Button = %ButtonDryStart

# source dir controls
@onready var label_source: Label = %LabelSource
@onready var option_source_drive: OptionButton = %OptionSourceDrive
@onready var button_select_source_dir: Button = %ButtonSelectSourceDir
@onready var check_source_drive_mode: CheckButton = %CheckSourceDriveMode

# backup dir controls
@onready var button_select_backup_dir: Button = %ButtonSelectBackupDir

@onready var output_label: RichTextLabel = %Output
@onready var file_viewer: FileViewer = %Files
@onready var tree_options: TreeOptions = %TreeOptions
@onready var file_dialog: FileDialog = %FileDialog

@onready var progress_bar: ProgressBar = %ProgressBar
@onready var progress_status_left: Label = %ProgressStatusLeft
@onready var progress_status_right: Label = %ProgressStatusRight

var config_file: ConfigFile = ConfigFile.new()

var tasks: M8ToolTaskList = null

var last_file: String = ""
var next_refresh_time: float = 0.0

var source_path: String = ""
var backup_path: String = ""

var is_dragging := false
var is_file_dialog_visible := false
var drag_offset := Vector2.ZERO


func _ready() -> void:
	self.config_file.load(CONFIG_PATH)

	self.check_source_drive_mode.button_pressed = self.config_file.get_value(
		"settings",
		"source_path_mode",
		SourcePathMode.DRIVE,
	)
	_set_source_drive(self.config_file.get_value("settings", "source_path", ""))
	_set_backup_dir(self.config_file.get_value("settings", "backup_path", ""))

	for option: String in ["backup", "optimize", "shrink", "clean"]:
		var default := self.tree_options.is_checked(option)
		var value: bool = self.config_file.get_value("settings", "%s_enabled" % option, default)
		self.tree_options.set_checked(option, value)
		self.tree_options.item_checked.connect(
			func(key: String, checked: bool) -> void:
				if key == option:
					self.config_file.set_value("settings", "%s_enabled" % key, checked),
		)

	get_window().content_scale_factor = DisplayServer.screen_get_scale()

	self.button_start.pressed.connect(self._on_pressed_start.bind(false))
	self.button_dry_start.pressed.connect(self._on_pressed_start.bind(true))
	self.button_select_backup_dir.pressed.connect(self._select_backup_dir)
	self.button_select_source_dir.pressed.connect(self._select_source_dir)
	self.button_close.pressed.connect(get_tree().quit)
	self.check_source_drive_mode.pressed.connect(self.refresh)

	self.file_dialog.canceled.connect(
		func() -> void:
			self.is_file_dialog_visible = false
			refresh(),
	)

	self.option_source_drive.item_selected.connect(
		func(_idx: int) -> void:
			_set_source_drive(_get_selected_source_drive_path()),
	)


func _input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		var e := event as InputEventMouseButton
		if e.button_index == MOUSE_BUTTON_LEFT:
			if e.pressed:
				is_dragging = true
				drag_offset = get_viewport().get_mouse_position()
			else:
				is_dragging = false

	elif event is InputEventMouseMotion and is_dragging:
		var e := event as InputEventMouseMotion
		if self.is_dragging:
			get_window().position += Vector2i(e.relative)


func _exit_tree() -> void:
	if self.tasks:
		self.tasks.kill()
	self.config_file.save(CONFIG_PATH)


func refresh() -> void:
	self.button_select_source_dir.disabled = self.is_file_dialog_visible
	self.button_select_backup_dir.disabled = self.is_file_dialog_visible

	if _get_source_path_mode() == SourcePathMode.DRIVE:
		self.option_source_drive.show()
		self.button_select_source_dir.hide()
		self.label_source.text = "source drive"
		_refresh_source_drive_paths()
	else:
		self.option_source_drive.hide()
		self.button_select_source_dir.show()
		self.label_source.text = "source directory"
		if self.source_path.is_empty():
			self.button_select_source_dir.text = "(choose directory...)"
		else:
			self.button_select_source_dir.text = self.source_path

	if self.backup_path.is_empty():
		self.button_select_backup_dir.text = "(choose directory...)"
	else:
		self.button_select_backup_dir.text = self.backup_path


## Update the list of drive paths and automatically select a drive.
func _refresh_source_drive_paths() -> void:
	var drive_names: Array = M8Tool.drives_list_names()
	var drive_paths: Array = M8Tool.drives_list_paths()

	self.option_source_drive.clear()
	for i in drive_names.size():
		var name: String = drive_names[i]
		var path: String = drive_paths[i]
		self.option_source_drive.add_item(name, i)
		self.option_source_drive.set_item_metadata(i, path)
		self.option_source_drive.set_item_icon(i, ICON_DRIVE)

	if self.option_source_drive.get_item_count() == 0:
		self.option_source_drive.add_item("(no drives found)", -1)
		self.option_source_drive.set_item_metadata(0, "")
		# self.option_source_drive.select(-1)
		self.option_source_drive.set_item_icon(0, ICON_DRIVE)
		self.option_source_drive.disabled = true
	else:
		self.option_source_drive.select(0)
		self.option_source_drive.disabled = false
		_set_source_drive(_get_selected_source_drive_path())


func _num_drives() -> int:
	return M8Tool.drives_list_names().size()


func _get_selected_source_drive_path() -> String:
	var index: int = self.option_source_drive.get_selected_id()
	if index < 0:
		return ""
	return self.option_source_drive.get_item_metadata(index)


func _select_backup_dir() -> void:
	self.button_select_source_dir.disabled = true
	self.button_select_backup_dir.disabled = true

	self.file_dialog.popup_centered()
	self.is_file_dialog_visible = true
	self.file_dialog.dir_selected.connect(
		func(dir: String) -> void:
			_set_backup_dir(dir)
			self.is_file_dialog_visible = false
			self.file_dialog.hide()
			refresh(),
		CONNECT_ONE_SHOT,
	)


func _select_source_dir() -> void:
	self.button_select_source_dir.disabled = true
	self.button_select_backup_dir.disabled = true

	self.file_dialog.popup_centered()
	self.is_file_dialog_visible = true
	self.file_dialog.dir_selected.connect(
		func(dir: String) -> void:
			_set_source_drive(dir)
			self.is_file_dialog_visible = false
			self.file_dialog.hide()
			refresh(),
		CONNECT_ONE_SHOT,
	)


func _set_backup_dir(dir: String) -> void:
	if !DirAccess.dir_exists_absolute(dir):
		return
	self.backup_path = dir
	self.config_file.set_value("settings", "backup_path", dir)

	refresh()


func _get_source_path_mode() -> SourcePathMode:
	if self.check_source_drive_mode.button_pressed:
		return SourcePathMode.DIRECTORY
	else:
		return SourcePathMode.DRIVE


func _set_source_drive(path: String) -> void:
	var mode := _get_source_path_mode()
	self.source_path = ""
	if mode == SourcePathMode.DRIVE:
		# check if drive exists
		for i in range(self.option_source_drive.get_item_count()):
			if self.option_source_drive.get_item_metadata(i) == path:
				self.option_source_drive.select(i)
				self.source_path = path
				self.config_file.set_value("settings", "source_path", path)
				self.config_file.set_value("settings", "source_path_mode", mode)

	elif mode == SourcePathMode.DIRECTORY:
		# check if dir exists
		if DirAccess.dir_exists_absolute(path):
			self.source_path = path
			self.config_file.set_value("settings", "source_path", path)
			self.config_file.set_value("settings", "source_path_mode", mode)

	# refresh()


func options_set_disabled(disabled: bool) -> void:
	if disabled:
		self.tree_options.mouse_filter = Control.MOUSE_FILTER_IGNORE
		self.tree_options.modulate = Color(1, 1, 1, 0.5)
	else:
		self.tree_options.mouse_filter = Control.MOUSE_FILTER_STOP
		self.tree_options.modulate = Color(1, 1, 1, 1)


func _process(delta: float) -> void:
	button_start.text = "cancel" if (self.tasks != null and self.tasks.is_running()) else "run tasks"
	button_dry_start.visible = false if (self.tasks != null and self.tasks.is_running()) else true

	self.button_start.disabled = self.source_path.is_empty() or self.backup_path.is_empty()
	self.button_dry_start.disabled = self.button_start.disabled
	options_set_disabled(self.button_start.disabled)

	self.next_refresh_time -= delta
	if self.next_refresh_time <= 0.0:
		refresh()
		self.next_refresh_time = 1.0

	if self.tasks:
		self.tasks.process()


func _on_receive_log(message: String) -> void:
	output_label.append_text("%s\n" % message)


func _on_receive_progress(dict: Dictionary) -> void:
	var percent: float = dict.get("percent", 0.0)
	var current_file: String = dict.get("file", "")
	var task_name: String = dict.get("task_name", "")
	var status: String = dict.get("status", "")
	var metadata: String = dict.get("metadata", "")

	_update_progress_bar(percent, current_file)

	if !current_file.is_empty() and current_file != last_file:
		last_file = current_file
		file_viewer.add_file(task_name, current_file, status, metadata)


func _update_progress_bar(percent: float, current_file: String) -> void:
	if percent >= 0.0:
		self.progress_bar.value = percent
		self.progress_status_left.text = "%.2f%%" % (percent)
		self.progress_status_right.text = current_file

	self.progress_bar.visible = !current_file.is_empty()
	self.progress_status_left.visible = !current_file.is_empty()
	self.progress_status_right.visible = !current_file.is_empty()


func _on_pressed_start(dry_run: bool) -> void:
	if self.tasks != null and self.tasks.is_running():
		self.tasks.kill()
		_update_progress_bar(0.0, "")
		self.output_label.append_text("task(s) cancelled\n")
	else:
		self.start_tasks(dry_run)


func start_tasks(dry_run: bool) -> void:
	self.output_label.clear()
	self.file_viewer.clear()

	if self.source_path.is_empty() or self.backup_path.is_empty():
		return

	self.output_label.append_text("[color=green]source path:[/color] %s\n" % self.source_path)
	self.output_label.append_text("[color=green]backup path:[/color] %s\n" % self.backup_path)

	self.tasks = M8ToolTaskList.create(self.source_path, self.backup_path, dry_run)
	# set task params
	self.tasks.set_optimize_whitelisted_dirs(self.tree_options.get_optimize_whitelisted_dirs())
	self.tasks.set_optimize_target_bit_depth(self.tree_options.get_optimize_target_bit_depth())
	self.tasks.set_optimize_target_sample_rate(self.tree_options.get_optimize_target_sample_rate())
	self.tasks.set_optimize_dual_mono_enabled(self.tree_options.get_optimize_from_dual_mono())

	self.tasks.received_progress.connect(self._on_receive_progress)
	self.tasks.received_log.connect(self._on_receive_log)

	# TODO: implement convert_other_formats
	self.tasks.set_shrink_whitelisted_dirs(self.tree_options.get_shrink_whitelisted_dirs())
	self.tasks.set_shrink_remove_common_prefixes(
		self.tree_options.get_shrink_remove_common_prefixes()
	)

	if self.tree_options.is_checked("backup"):
		self.tasks.add_backup_task()

	if self.tree_options.is_checked("optimize"):
		self.tasks.add_optimize_task()

	if self.tree_options.is_checked("shrink"):
		self.tasks.add_shrink_task()

	if self.tree_options.is_checked("clean"):
		self.tasks.add_clean_task()

	var task_names := self.tasks.get_task_names()

	self.output_label.append_text(
		"[color=green]created task list:[/color] %s\n" % " -> ".join(task_names)
	)
	self.output_label.append_text("[color=gray]--------------------------------[/color]\n")

	self.tasks.start()
