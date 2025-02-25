from colorthief import ColorThief
import sys

c = ColorThief(sys.argv[1])
palette = [c.get_color(quality=2)]
palette.extend(c.get_palette(color_count=int(sys.argv[2]) - 1, quality=2))
[print("%0.2X%0.2X%0.2X" % p) for p in palette]
