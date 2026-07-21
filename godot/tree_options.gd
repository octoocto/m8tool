class_name TreeOptions
extends Tree

signal item_checked(key: String, checked: bool)

var item_map: Dictionary[StringName, TreeItem] = { }


func _ready() -> void:
	var root := self.create_item()

	_create_item_check(root, "backup", "full backup", true)
	_create_item_check(root, "convert", "convert samples", true)
	_create_item_check(root, "shrink", "minimize paths to samples", true)
	_create_item_check(root, "clean", "clean", true)

	_create_item_text("convert", "convert_whitelist", "list of subdirectories to scan", "samples,packs")
	_create_item_option("convert", "convert_bit_depth", "target bit depth (bits)", ["keep", "8", "16", "24", "32"], 2)
	_create_item_option("convert", "convert_sample_rate", "target sample rate (Hz)", ["keep", "44100", "48000"], 1)
	_create_item_check("convert", "convert_other", "convert non-wav files to wav", true)
	_create_item_check("convert", "convert_mono", "convert dual mono samples to mono", true)

	_create_item_text("shrink", "shrink_whitelist", "list of subdirectories to scan", "samples,packs")
	_create_item_check("shrink", "shrink_remove_common_prefixes", "remove common prefixes in name", true)

	self.item_map["backup"].set_tooltip_text(0, "creates an incremental backup of the drive using rsync")
	self.item_map["convert"].set_tooltip_text(0, "converts samples to a target bit depth and sample rate using ffmpeg")
	self.item_map["shrink"].set_tooltip_text(0, "shortens sample paths as much as possible")
	self.item_map["clean"].set_tooltip_text(0, "removes extra files from the drive")

	if M8Tool.which("rsync") == "":
		self.item_map["backup"].set_checked(0, false)
		self.item_map["backup"].set_editable(0, false)
		self.item_map["backup"].set_text(0, "backup drive (rsync not found)")

	if M8Tool.which("ffmpeg") == "" or M8Tool.which("ffprobe") == "":
		self.item_map["convert"].set_checked(0, false)
		self.item_map["convert"].set_editable(0, false)
		self.item_map["convert"].set_text(0, "convert samples (ffmpeg and/or ffprobe not found)")
		var child := self.item_map["convert"].get_first_child()
		while child != null:
			child.set_editable(0, false)
			child.set_editable(1, false)
			child = child.get_next()

	var child := root.get_first_child()
	while child != null:
		child.set_collapsed(true)
		child = child.get_next()

	self.item_edited.connect(self._on_item_edited)


func _on_item_edited() -> void:
	var item := self.get_edited()
	var key: String = item.get_metadata(0)
	if item.get_cell_mode(0) == TreeItem.CELL_MODE_CHECK:
		self.item_checked.emit(key, item.is_checked(0))


func is_checked(key: String) -> bool:
	assert(key in self.item_map, "expected key to exist: %s" % key)
	if key not in self.item_map:
		return false
	return self.item_map[key].is_checked(0)


func set_checked(key: String, checked: bool) -> void:
	if key not in self.item_map:
		return
	self.item_map[key].set_checked(0, checked)


func get_shrink_whitelisted_dirs() -> PackedStringArray:
	if "shrink_whitelist" not in self.item_map:
		return PackedStringArray()
	var text := self.item_map["shrink_whitelist"].get_text(1)
	var dirs := text.split(",", false)
	for i in range(dirs.size()):
		dirs[i] = dirs[i].strip_edges()
		dirs[i] = dirs[i].rstrip("/\\")
		dirs[i] = dirs[i].lstrip("/\\")
	return PackedStringArray(dirs)


func get_shrink_remove_common_prefixes() -> bool:
	return is_checked("shrink_remove_common_prefixes")


func get_convert_whitelisted_dirs() -> PackedStringArray:
	if "convert_whitelist" not in self.item_map:
		return PackedStringArray()
	var text := self.item_map["convert_whitelist"].get_text(1)
	var dirs := text.split(",", false)
	for i in range(dirs.size()):
		dirs[i] = dirs[i].strip_edges()
		dirs[i] = dirs[i].rstrip("/\\")
		dirs[i] = dirs[i].lstrip("/\\")
	return PackedStringArray(dirs)


func get_convert_target_bit_depth() -> int:
	if "convert_bit_depth" not in self.item_map:
		return 0

	match int(self.item_map["convert_bit_depth"].get_range(1)):
		1:
			return 8
		2:
			return 16
		3:
			return 24
		4:
			return 32
		_:
			return 0


func get_convert_target_sample_rate() -> int:
	if "convert_sample_rate" not in self.item_map:
		return 0

	match int(self.item_map["convert_sample_rate"].get_range(1)):
		1:
			return 44100
		2:
			return 48000
		_:
			return 0


func get_convert_from_dual_mono() -> bool:
	if "convert_mono" not in self.item_map:
		return false
	return self.item_map["convert_mono"].is_checked(0)


func get_convert_other_formats() -> bool:
	if "convert_other" not in self.item_map:
		return false
	return self.item_map["convert_other"].is_checked(0)


func _create_item(parent: Variant, key: StringName) -> TreeItem:
	var parent_item: TreeItem
	if parent is String:
		assert(parent in self.item_map)
		parent_item = self.item_map[parent]
	else:
		assert(parent is TreeItem)
		parent_item = parent

	var item := self.create_item(parent_item)
	item.set_metadata(0, key)
	item.set_selectable(0, false)
	self.item_map[key] = item
	return item


func _create_item_rangei(parent: Variant, key: String, text: String, range_min: int, range_max: int, default: int) -> TreeItem:
	var item := _create_item(parent, key)
	item.set_text(0, text)
	item.set_cell_mode(1, TreeItem.CELL_MODE_RANGE)
	item.set_range_config(1, range_min, range_max, 1, false)
	item.set_range(1, default)
	item.set_editable(1, true)
	return item


func _create_item_check(parent: Variant, key: String, text: String, default: bool) -> TreeItem:
	var item := _create_item(parent, key)
	item.set_cell_mode(0, TreeItem.CELL_MODE_CHECK)
	item.set_text(0, text)
	item.set_checked(0, default)
	item.set_editable(0, true)
	return item


func _create_item_option(parent: Variant, key: String, text: String, item_options: PackedStringArray, default: int) -> TreeItem:
	var item := _create_item(parent, key)
	item.set_cell_mode(1, TreeItem.CELL_MODE_RANGE)
	item.set_text(0, text)
	item.set_text(1, ",".join(item_options))
	item.set_range(1, default)
	item.set_editable(1, true)
	return item


func _create_item_text(parent: Variant, key: String, text: String, default: String) -> TreeItem:
	var item := _create_item(parent, key)
	item.set_cell_mode(1, TreeItem.CELL_MODE_STRING)
	item.set_text(0, text)
	item.set_text(1, default)
	item.set_editable(1, true)
	return item
