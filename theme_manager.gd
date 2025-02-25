extends MarginContainer

var colors: Array[Color]

@export var override_contrast := false

@export var value_min := 0.4
@export var value_max := 0.7

func _ready():
    %ButtonOpenImage.pressed.connect(func() -> void:
        %FileDialog.visible = true
    )

    %FileDialog.file_selected.connect(func(path: String) -> void:
        %LineEditImage.text = path.get_file()
        %TextureRectImage.texture = ImageTexture.create_from_image(Image.load_from_file(path))
        var num_colors := 13

        match (%OptionButtonNumColors.selected):
            0:
                num_colors = 4

        colors = _get_palette(path, num_colors)

        for i in 13:
            var node_path := "%" + "Color%d" % (i + 1)
            var node: ColorPickerButton = get_node(node_path)
            node.color = colors[i]
            node.color_changed.emit(colors[i])
    )

    for i in 13:
        var node_path := "%" + "Color%d" % (i + 1)
        var node: ColorPickerButton = get_node(node_path)
        var label: Label = node.get_node("Label")
        node.color_changed.connect(func(color: Color) -> void:
            label.text = color.to_html(false).to_upper()
            if color.get_luminance() < 0.5:
                label.set("theme_override_colors/font_color", color.lightened(0.8))
            else:
                label.set("theme_override_colors/font_color", color.darkened(0.8))
        )

    %ButtonPushToM8.pressed.connect(func() -> void:
        var devices := M8GD.list_devices()
        var m8gd: M8GD = %M8GD
        if devices.size() > 0:
            if m8gd.connect(devices[0]):
                for i in 13:
                    _push_to_m8_print("Pushing color %d/13" % [i + 1])
                    m8gd.set_theme_color(i, colors[i])
                    await get_tree().create_timer(0.2).timeout
                _push_to_m8_print("Success!")
                m8gd.disconnect()
            else:
                _push_to_m8_print("Failed to connect to M8! (is another app using it?)")
        else:
            _push_to_m8_print("No M8 devices found!")
    )

var tween_push_to_m8_print: Tween

func _push_to_m8_print(text: String) -> void:
    %ButtonPushToM8.text = text
    if tween_push_to_m8_print:
        tween_push_to_m8_print.kill()
    tween_push_to_m8_print = create_tween()
    tween_push_to_m8_print.tween_interval(2.0)
    tween_push_to_m8_print.tween_callback(func() -> void:
        %ButtonPushToM8.text = "Push colors to M8"
    )

func _get_palette(image_path: String, num_colors := 13) -> Array[Color]:
    var output = []
    var colors: Array[Color] = []
    OS.execute("python", ["get_palette.py", image_path, num_colors], output, true)
    var stdout: String = output[0]
    for color_code in stdout.split("\n", false):
        var color := Color.from_string(color_code.strip_edges(), Color.WHITE)
        colors.append(color)

    match num_colors:
        4:
            while colors.size() < 4: colors.append(colors[-1])

            colors.sort_custom(func(a: Color, b: Color) -> bool:
                return a.get_luminance() < b.get_luminance()
            )

            if override_contrast:
                # var v_lo := colors[0].v
                var v_lo: float = lerp(colors[0].v, 0.25, 0.5)
                var v_hi: float = colors[3].v

                colors[0].v = lerp(v_lo, v_hi, 0)
                colors[1].v = lerp(v_lo, v_hi, 0.1)
                colors[2].v = lerp(v_lo, v_hi, 0.4)

            colors = [
                colors[0],
                colors[1],
                colors[2],
                colors[2],
                colors[3],
                colors[3],
                colors[2],
                colors[3],
                colors[2],
                colors[2],
                colors[1],
                colors[1],
                colors[1],
            ]
        _:
            while colors.size() < 13: colors.append(colors[-1])

            colors.sort_custom(func(a: Color, b: Color) -> bool:
                return a.get_luminance() < b.get_luminance()
            )

            colors = _array_swap(colors, 7, 12)
            colors = _array_swap(colors, 4, 11)
            colors = _array_swap(colors, 5, 10)
            colors = _array_swap(colors, 3, 9)
            colors = _array_swap(colors, 2, 8)

            colors = _array_swap(colors, 8, 12)

            if abs(colors[1].h - colors[2].h) < abs(colors[1].h - colors[11].h) or colors[11].s > colors[2].s:
                colors = _array_swap(colors, 2, 11)

            # adjust contrast

            if override_contrast:
                var v_lo: float = lerp(colors[0].v, value_min, 0.9)
                var v_hi: float = lerp(colors[12].v, value_max, 0.9)

                print("color value range: %.2f - %.2f" % [v_lo, v_hi])

                colors[0].v = v_lo
                colors[1].v = lerp(v_lo, v_hi, 0.4)
                colors[2].v = lerp(v_lo, v_hi, 0.4)
                colors[3].v = lerp(v_lo, v_hi, 0.8)
                colors[4].v = lerp(v_lo, v_hi, 0.8)
                colors[5].v = lerp(v_lo, v_hi, 0.8)
                colors[6].v = v_hi
                colors[7].v = v_hi
                colors[8].v = v_hi

                # scope/slider
                colors[9].v = max(0.5, colors[9].v)

                # meter
                colors[10].v = max(0.5, colors[10].v)
                colors[11].v = max(0.6, colors[11].v)
                colors[12].v = max(0.7, colors[12].v)

            # sort meter colors by hue
            var meter_colors := colors.slice(10)
            meter_colors.sort_custom(func(a: Color, b: Color) -> bool:
                return a.h < b.h
            )

            colors[10] = meter_colors[0]
            colors[11] = meter_colors[1]
            colors[12] = meter_colors[2]


    assert(colors.size() == 13)
    return colors

func _array_swap(arr: Array, a: int, b: int) -> Array:
    var tmp: Variant = arr[a]
    arr[a] = arr[b]
    arr[b] = tmp
    return arr
