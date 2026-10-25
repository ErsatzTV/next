#!/usr/bin/env python3
"""Writes a minimal PGS (.sup) file: one opaque white box shown from 0 s to 10 s.

ffmpeg can't render text into bitmap subtitles, so the image-subtitle fixture is
built by hand from the segment layout in the Blu-ray PGS spec.
"""
import struct
import sys

WIDTH, HEIGHT = 1920, 1080
BOX_W, BOX_H = 400, 80
BOX_X, BOX_Y = (WIDTH - BOX_W) // 2, HEIGHT - BOX_H - 60
END_S = 10


def segment(pts_s, kind, payload):
    pts = int(pts_s * 90000)
    return b"PG" + struct.pack(">IIBH", pts, 0, kind, len(payload)) + payload


def pcs(state, objects):
    body = struct.pack(">HHBHBBBB", WIDTH, HEIGHT, 0x10, 0, state, 0, 0, len(objects))
    for x, y in objects:
        body += struct.pack(">HBBHH", 0, 0, 0, x, y)
    return body


def wds():
    return struct.pack(">BBHHHH", 1, 0, BOX_X, BOX_Y, BOX_W, BOX_H)


def pds():
    # palette id 0, version 0; entry 1 = opaque white (Y, Cr, Cb, A)
    return struct.pack(">BB", 0, 0) + struct.pack(">BBBBB", 1, 235, 128, 128, 255)


def rle_line(color, length):
    # 00 11LLLLLL LLLLLLLL CC: run of `length` pixels of `color`, then end of line
    return bytes([0, 0xC0 | (length >> 8), length & 0xFF, color, 0, 0])


def ods():
    data = struct.pack(">HH", BOX_W, BOX_H) + rle_line(1, BOX_W) * BOX_H
    # object id 0, version 0, first+last in sequence, 24-bit length incl. width/height
    return struct.pack(">HBB", 0, 0, 0xC0) + len(data).to_bytes(3, "big") + data


out = b"".join(
    [
        segment(0, 0x16, pcs(0x80, [(BOX_X, BOX_Y)])),
        segment(0, 0x17, wds()),
        segment(0, 0x14, pds()),
        segment(0, 0x15, ods()),
        segment(0, 0x80, b""),
        segment(END_S, 0x16, pcs(0x00, [])),
        segment(END_S, 0x17, wds()),
        segment(END_S, 0x80, b""),
    ]
)
with open(sys.argv[1], "wb") as f:
    f.write(out)
