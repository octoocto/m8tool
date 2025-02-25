class_name SettingBase extends PanelContainer

signal value_changed(value: Variant)


@export var enabled := true:
    set(value):
        enabled = value
        _update()

@export var setting_name := "":
    set(value):
        setting_name = value
        _update()

@export var setting_name_min_width := 160:
    set(value):
        setting_name_min_width = value
        _update()

@export var setting_name_indent := 0:
    set(value):
        setting_name_indent = max(0, value)
        _update()

@export_node_path("SettingBase") var parent_setting := NodePath("")

@export_enum("Disable", "Hide") var parent_setting_mode: int

@export var parent_setting_invert := false


## If true, one of the [init] methods has been called.
var _is_initialized := false

## If true, fire the [value_changed] signal when [value] is set.
var _value_changed_signal_enabled := true

## Set to a Callable that gets the initial value of [value].
var _value_init_fn: Callable

## A Callable that maps the parent setting's value to a bool,
## which decides whether to disable/hide this setting.
var _parent_value_check_fn: Callable


func _ready() -> void:
    if parent_setting:
        get_node(parent_setting).value_changed.connect(func(_value: Variant) -> void:
            _check_parent()
        )
        _check_parent()

func _make_custom_tooltip(for_text: String) -> Object:
    if for_text:
        var tooltip := preload("res://tooltip.tscn").instantiate()
        tooltip.text = for_text
        return tooltip
    else:
        return null

func set_value_no_signal(value: Variant) -> void:
    _value_changed_signal_enabled = false
    set("value",value)
    _value_changed_signal_enabled = true


func init(value_init_fn: Variant, value_changed_fn: Callable) -> void:
    assert("value" in self)
    assert(!_is_initialized, "This setting has already been initialized: %s" % name)

    if value_init_fn is Callable:
        _value_init_fn = value_init_fn
    elif value_init_fn == null:
        var value: Variant = get("value")
        _value_init_fn = func() -> Variant: return value
    else:
        _value_init_fn = func() -> Variant: return value_init_fn

    value_changed.connect(value_changed_fn)
    set_value_no_signal(_value_init_fn.call())
    force_update()

    _is_initialized = true

##
## Re-initialize this setting to an initial value and emits [value_changed].
## This is useful when a profile or scene has just been loaded, and
## the initial value could be different.
##
func reinit(emit_value_changed := true) -> void:
    assert(_is_initialized and _value_init_fn, "This setting has not been initialized yet: %s" % name)
    if emit_value_changed:
        set("value", _value_init_fn.call())
    else:
        set_value_no_signal(_value_init_fn.call())
    # print("%s: reinitializing value" % name)

##
## Uninitialize this setting, disconnecting all signal connections.
## Note: the value won't change.
##
func uninit() -> void:
    if _is_initialized:
        _clear_signals()
        _value_init_fn = func() -> void: return
        _is_initialized = false

##
## Links this setting to enable/disable a control node.
##
func connect_to_enable(control: Control) -> SettingBase:
    var dst_property: String
    var invert := false

    if control is Slider:
        dst_property = "editable"
    elif control is Button:
        dst_property = "disabled"
        invert = true
    elif control is SettingBase:
        dst_property = "enabled"
    else:
        assert(false)

    assert(dst_property in control)

    value_changed.connect(func(value: Variant) -> void:
        control.set(dst_property, bool(value) if !invert else !bool(value))
    )

    control.set(dst_property, bool(get("value")) if !invert else !bool(get("value")))

    return self

##
## Links this setting to unhide/hide a control node.
##
func connect_to_visible(control: Control, check_fn: Variant = null) -> SettingBase:
    assert(check_fn == null or check_fn is Callable)
    var invert := false

    var _check := func() -> bool:
        var bool_value: bool
        if check_fn:
            bool_value = check_fn.call(get("value"))
        else:
            bool_value = bool(get("value"))
        return !bool_value if invert else bool_value

    value_changed.connect(func(_value: Variant) -> void:
        control.visible = _check.call()
    )

    control.visible = _check.call()

    return self

func _update() -> void:
    assert(false, "not implemented")

func _check_parent() -> void:
    var can_enable := true
    if parent_setting:
        var setting := get_node(parent_setting)
        if _parent_value_check_fn:
            can_enable = _parent_value_check_fn.call(setting.value)
        else:
            can_enable = bool(setting.value)

    # invert if needed
    can_enable = !can_enable if parent_setting_invert else can_enable

    if parent_setting_mode == 0:
        enabled = can_enable
    else:
        visible = can_enable

func force_update() -> void:
    if _value_changed_signal_enabled:
        value_changed.emit(get("value"))

func _clear_signals() -> void:
    for conn: Dictionary in value_changed.get_connections():
        value_changed.disconnect(conn.callable)
