## Manages the Files tab in the UI, which displays the list of files that were
## processed during conversion, along with their status and any relevant metadata.
class_name FileViewer
extends VBoxContainer

const ICON_GOOD := preload("res://icon/StatusSuccess.png")
const ICON_CONVERTED := preload("res://icon/StatusConverted.png")
const ICON_SKIPPED := preload("res://icon/StatusSkip.png")

@onready var tree: Tree = %FileTree

var item_map: Dictionary[StringName, TreeItem] = { }
var right_col_min_width: int = 0


func _ready() -> void:
	self.tree.set_hide_root(true)
	self.tree.columns = 2
	self.tree.select_mode = Tree.SELECT_ROW
	self.tree.set_column_expand(0, true)
	self.tree.set_column_expand(1, false)
	self.clear()


func clear() -> void:
	self.tree.clear()
	self.item_map.clear()
	self.tree.create_item() # root item


func add_file(section: String, file_path: String, status: String, metadata: String = "") -> void:
	if section not in self.item_map:
		var c := self.tree.get_root().get_first_child()
		while c != null:
			c.set_collapsed(true)
			c = c.get_next()

		var section_item := tree.create_item()
		self.item_map[section] = section_item

		section_item.set_metadata(0, section)
		section_item.set_text(0, section)
		section_item.set_selectable(0, false)
		section_item.set_custom_stylebox(0, self.get_theme_stylebox("panel", "TreeItemHeader"))
		section_item.set_custom_stylebox(1, self.get_theme_stylebox("panel", "TreeItemHeader"))
		section_item.set_custom_font(0, self.get_theme_font("font", "TreeItemHeader"))

	var section_item: TreeItem = self.item_map[section]

	if status == "skipped":
		return

	var file_item := tree.create_item(section_item)
	file_item.set_text(0, file_path)
	if !metadata.is_empty():
		var text_size := get_theme_default_font().get_string_size(
			metadata,
			HORIZONTAL_ALIGNMENT_LEFT,
			-1,
			get_theme_default_font_size(),
		)
		self.right_col_min_width = max(
			int(text_size.x) + 16,
			self.right_col_min_width,
		)
		self.tree.set_column_custom_minimum_width(1, self.right_col_min_width)
		file_item.set_text(1, metadata)
		file_item.set_text_alignment(1, HORIZONTAL_ALIGNMENT_RIGHT)

	var section_title: String = section_item.get_metadata(0)
	section_item.set_text(0, "%s (%d files)" % [section_title, section_item.get_child_count()])

	match status:
		"good":
			file_item.set_icon(0, ICON_GOOD)
			file_item.set_custom_color(0, Color(0.5, 0.5, 0.5))
		"converted":
			file_item.set_icon(0, ICON_CONVERTED)
		"skipped":
			file_item.set_icon(0, ICON_SKIPPED)
			file_item.set_custom_color(0, Color(0.5, 0.5, 0.5))
