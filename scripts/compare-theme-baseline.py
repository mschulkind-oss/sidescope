#!/usr/bin/env python3
"""Exact default-theme pixel check. Use synthetic full-wide fixture captures only.

Excludes new footer chrome and the fixed condition-label rectangle, nothing else.
PNG decoding is stdlib-only; this is test tooling, not an application dependency.
"""
import struct
import sys
import zlib


def pixels(path):
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    offset, compressed = 8, bytearray()
    while offset < len(data):
        size = struct.unpack(">I", data[offset:offset + 4])[0]
        kind = data[offset + 4:offset + 8]
        chunk = data[offset + 8:offset + 8 + size]
        if kind == b"IHDR":
            width, height, depth, color, _, _, interlace = struct.unpack(">IIBBBBB", chunk)
            assert depth == 8 and color in (2, 6) and interlace == 0
        if kind == b"IDAT":
            compressed.extend(chunk)
        offset += size + 12
    channels = 3 if color == 2 else 4
    stride = width * channels
    raw = zlib.decompress(compressed)
    rows, previous = [], bytearray(stride)
    for y in range(height):
        start = y * (stride + 1)
        kind = raw[start]
        row = bytearray(raw[start + 1:start + 1 + stride])
        for x in range(stride):
            a = row[x - channels] if x >= channels else 0
            b = previous[x]
            c = previous[x - channels] if x >= channels else 0
            if kind == 1:
                predictor = a
            elif kind == 2:
                predictor = b
            elif kind == 3:
                predictor = (a + b) // 2
            elif kind == 4:
                p = a + b - c
                distances = (abs(p - a), abs(p - b), abs(p - c))
                predictor = (a, b, c)[distances.index(min(distances))]
            else:
                assert kind == 0
                predictor = 0
            row[x] = (row[x] + predictor) & 255
        rows.append([bytes(row[x:x + 3]) for x in range(0, stride, channels)])
        previous = row
    return width, height, rows


if __name__ == "__main__":
    before_w, before_h, before = pixels(sys.argv[1])
    after_w, after_h, after = pixels(sys.argv[2])
    assert (before_w, before_h) == (after_w, after_h) == (640, 1415)
    differences, checked = [], 0
    for y in range(1350):
        for x in range(640):
            if 128 <= x < 312 and 615 <= y < 650:
                continue  # Unavoidable current-condition nonwrapping repair.
            checked += 1
            if before[y][x] != after[y][x]:
                differences.append((x, y))
    print(f"{checked} pixels checked; {len(differences)} differences outside condition/footer exclusions")
    if differences:
        print(f"First differing pixels: {differences[:10]}")
        sys.exit(1)
