#!/usr/bin/env python3
"""Adds an ATSC A53 caption SEI to every picture of an Annex B H.264 stream.

ffmpeg has no source that produces caption side data, so the captions are written
here. The cc_data is padding only (one CEA-608 null pair, 19 DTVCC padding
triplets), which is enough for decoders and encoders to see and carry A53.

usage: insert_a53cc.py <in.h264> <out.h264>
"""
import re
import sys

CC_COUNT = 20


def t35_payload():
    triplets = bytes([0xFC, 0x80, 0x80]) + bytes([0xFA, 0x00, 0x00]) * (CC_COUNT - 1)
    return (
        bytes([0xB5, 0x00, 0x31])  # country code USA, provider ATSC
        + b"GA94"
        + bytes([0x03, 0x40 | CC_COUNT, 0xFF])  # cc_data, process_cc_data_flag, em_data
        + triplets
        + bytes([0xFF])  # marker_bits
    )


def escape(rbsp):
    out = bytearray()
    zeros = 0
    for b in rbsp:
        if zeros >= 2 and b <= 3:
            out.append(3)
            zeros = 0
        out.append(b)
        zeros = zeros + 1 if b == 0 else 0
    return bytes(out)


def sei_nal():
    payload = t35_payload()
    # payloadType 4 = user_data_registered_itu_t_t35; both fit in one byte
    rbsp = bytes([4, len(payload)]) + payload + bytes([0x80])
    return b"\x00\x00\x00\x01" + bytes([0x06]) + escape(rbsp)


def main():
    data = open(sys.argv[1], "rb").read()
    starts = [m.end() for m in re.finditer(b"\x00\x00\x01", data)]
    sei = sei_nal()
    out = bytearray()
    for i, start in enumerate(starts):
        end = starts[i + 1] - 3 if i + 1 < len(starts) else len(data)
        # a NAL never ends in 0x00; trailing zeros belong to the next 4-byte start code
        nal = data[start:end].rstrip(b"\x00")
        # x264 writes one slice per picture here, so every VCL NAL starts a picture
        if nal[0] & 0x1F in (1, 5):
            out += sei
        out += b"\x00\x00\x00\x01" + nal
    open(sys.argv[2], "wb").write(out)


main()
