extends PanelContainer

var pressed := false

func _gui_input(event):
    if event is InputEventMouseButton and event.pressed:
        pressed = true

    elif event is InputEventMouseButton and !event.pressed:
        pressed = false

    if event is InputEventMouseMotion and pressed:
        get_window().position += Vector2i(event.relative)