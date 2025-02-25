class_name OperationConvertBPS extends FileOperation

var needed: bool = false
var old_bps: int
var target_bps := 16
var shell: Shell
var filepath: String

func _init(shell_: Shell, path: String) -> void:
    shell = shell_
    filepath = path

    old_bps = shell.ffprobe_get_bits_per_sample(path)
    if old_bps > target_bps:
        needed = true

func is_operation_needed() -> bool:
    return needed

func get_description() -> String:
    return "CONVERT BIT DEPTH: %d -> %d" % [old_bps, target_bps]

func perform() -> void:
    return
