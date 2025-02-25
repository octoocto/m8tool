extends CheckBox

func _make_custom_tooltip(for_text: String) -> Object:
    if for_text:
        var tooltip := preload("res://tooltip.tscn").instantiate()
        tooltip.text = for_text
        return tooltip
    else:
        return null
