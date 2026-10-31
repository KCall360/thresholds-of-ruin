"""Read native RGB/RGBA8 screenshots without Pillow."""
import struct
import zlib
from statistics import median_low


def decode_png(data):
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG screenshot")
    width, height, depth, kind, compression, filtering, interlace = struct.unpack(">IIBBBBB", data[16:29])
    if depth != 8 or kind not in (2, 6) or compression or filtering or interlace or not width or not height:
        raise ValueError("expected noninterlaced RGB/RGBA8")
    channels = 3 if kind == 2 else 4
    compressed = bytearray()
    offset = 8
    while offset < len(data):
        size = struct.unpack(">I", data[offset:offset + 4])[0]
        if data[offset + 4:offset + 8] == b"IDAT":
            compressed.extend(data[offset + 8:offset + 8 + size])
        offset += size + 12
    raw = zlib.decompress(compressed)
    stride = width * channels
    if len(raw) != height * (stride + 1):
        raise ValueError("incomplete screenshot rows")
    rows = []
    previous = bytearray(stride)
    for y in range(height):
        start = y * (stride + 1)
        filter_kind = raw[start]
        row = bytearray(raw[start + 1:start + 1 + stride])
        for x in range(stride):
            left = row[x - channels] if x >= channels else 0
            above = previous[x]
            corner = previous[x - channels] if x >= channels else 0
            if filter_kind == 0:
                predictor = 0
            elif filter_kind == 1:
                predictor = left
            elif filter_kind == 2:
                predictor = above
            elif filter_kind == 3:
                predictor = (left + above) // 2
            elif filter_kind == 4:
                estimate = left + above - corner
                a, b, c = abs(estimate - left), abs(estimate - above), abs(estimate - corner)
                predictor = left if a <= b and a <= c else above if b <= c else corner
            else:
                raise ValueError("unsupported PNG row filter")
            row[x] = (row[x] + predictor) & 255
        rows.append(row)
        previous = row
    return width, height, channels, rows


def dark_fraction(data, bounds, threshold=80):
    """Near-black pixel fraction in a normalized, explicitly selected region."""
    width, height, channels, rows = decode_png(data)
    x0, y0, x1, y1 = bounds
    if not (0 <= x0 < x1 <= 1 and 0 <= y0 < y1 <= 1):
        raise ValueError("region must lie inside screenshot")
    values = [max(rows[y][x * channels:x * channels + 3])
              for y in range(int(y0 * height), int(y1 * height))
              for x in range(int(x0 * width), int(x1 * width))]
    if not values:
        raise ValueError("empty screenshot region")
    return sum(value < threshold for value in values) / len(values)


def portrait_pixels(data, bounds):
    """Return bright/color sample count and the coherent figure's height fraction.

    The native portrait has a near-black clear background and no UI inside its
    image bounds. Keep the existing every-other-pixel/color evidence, but do not
    mistake colored clothing's height for the complete character's height.
    Grow through contrasting dark/neutral pixels only when connected to a
    substantial colored body. Never union disconnected marks or image borders.
    This is a fixture-specific screenshot oracle, not general object detection.
    """
    width, height, channels, rows = decode_png(data)
    left, top, right, bottom = bounds
    if not (0 <= left < right <= 1 and 0 <= top < bottom <= 1):
        raise ValueError("portrait bounds must lie inside screenshot")
    xs = range(int((left + 0.02) * width), int((right - 0.02) * width), 2)
    ys = range(int((top + 0.02) * height), int((bottom - 0.02) * height), 2)
    nx, ny = len(xs), len(ys)
    if min(nx, ny) < 3:
        raise ValueError("portrait inset must contain an interior")
    pixels = [tuple(rows[y][x * channels:x * channels + 3]) for y in ys for x in xs]
    colored = [max(rgb) > 55 and max(rgb) - min(rgb) > 10 for rgb in pixels]
    # Sampling all four sides avoids taking clothing at a single corner as the
    # background. Uniform bright panes cannot impersonate the expected clear.
    perimeter = [pixels[y * nx + x] for y in range(ny) for x in range(nx)
                 if x in (0, nx - 1) or y in (0, ny - 1)]
    background = tuple(median_low(rgb[c] for rgb in perimeter) for c in range(3))
    if max(background) > 20:
        return sum(colored), 0.0
    # A 12-code-value contrast clears the observed 1–2 value background dither
    # without requiring saturated or brightly lit trousers/boots.
    foreground = {i for i, rgb in enumerate(pixels)
                  if max(abs(a - b) for a, b in zip(rgb, background)) > 12}
    largest = (0, 0)
    while foreground:
        first = foreground.pop()
        pending = [first]
        size, seeds, touches_edge = 0, 0, False
        low, high = ny, 0
        while pending:
            i = pending.pop()
            y, x = divmod(i, nx)
            size += 1
            seeds += colored[i]
            low, high = min(low, y), max(high, y)
            touches_edge |= x in (0, nx - 1) or y in (0, ny - 1)
            for dy in (-1, 0, 1):
                for dx in (-1, 0, 1):
                    xx, yy = x + dx, y + dy
                    if 0 <= xx < nx and 0 <= yy < ny:
                        neighbor = yy * nx + xx
                        if neighbor in foreground:
                            foreground.remove(neighbor)
                            pending.append(neighbor)
        if seeds > 1000 and not touches_edge and size > largest[0]:
            largest = (size, (ys[high] - ys[low]) / ((bottom - top) * height))
    return sum(colored), largest[1]
