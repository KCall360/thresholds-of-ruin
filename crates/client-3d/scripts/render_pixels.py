"""Read native RGB/RGBA8 screenshots without Pillow."""
import struct
import zlib


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
