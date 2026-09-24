#!/usr/bin/env python3
"""Generator for the P4.108 animated-input fixtures in this directory.

The committed BYTES are canonical; this script is how they were made and how to
re-make them. Only the standard library is used for the GIFs, the PNG frames and
the APNG, so those come out byte-for-byte identical on any machine
(`python3 generate.py --check` verifies that against the committed files). The
two WebP fixtures need libwebp's command-line tools (`img2webp`, `cwebp`), whose
output depends on the libwebp version (1.6.0 made the committed ones), so they
are rebuilt only with `--webp`, and a rebuilt one is NOT expected to match the
committed bytes — which is why the bytes are committed at all.

    python3 generate.py            # write the GIFs + the APNG
    python3 generate.py --check    # verify them byte-for-byte, write nothing
    python3 generate.py --webp     # ALSO rebuild the two WebPs (needs libwebp)

The fixtures (read only by `normalize_blob_image_equivalence` and the host
codec's unit tests):

* `anim-2frame.gif`     32x24, TWO image descriptors (a red then a blue frame),
                        NETSCAPE2.0 infinite loop, a GCE per frame (100 cs).
* `still-large.gif`     64x48, ONE descriptor, a four-colour stripe pattern —
                        large enough that both encoders clearly SHRINK it (a
                        2x2 GIF GROWS under sharp, 51 -> 66 bytes).
* `still-with-commas.gif` 32x24, ONE descriptor, preceded by a comment
                        extension full of 0x2C bytes — the byte that
                        introduces an image descriptor, so a naive byte scan
                        would count frames that are not there.
* `anim-2frame.webp`    `img2webp -loop 0 -d 100 -lossy f1.png f2.png`: an
                        animated WebP with two ANMF frames.
* `one-anmf.webp`       hand-assembled: VP8X with the ANIMATION bit set, an
                        ANIM chunk, and exactly ONE ANMF wrapping the VP8
                        bitstream `cwebp` made from f1.png. (`webpmux` given a
                        single frame collapses it to a plain VP8 file, so this
                        shape has to be built by hand.) sharp reads it as ONE
                        page — a still.
* `anim-2frame.apng`    hand-assembled APNG (acTL num_frames=2, fcTL + IDAT,
                        fcTL + fdAT). sharp does not decode APNG animation
                        (format=png, a still), so it must never be declined.
* `anim-corrupt2.gif`   (P4.112) `anim-2frame.gif`'s shape with the SECOND
                        frame's LZW stream corrupt: a clear code followed by
                        code 7, which no 4-colour dictionary holds (the next
                        free entry after a clear is 6). The first frame is
                        intact. Made to MEASURE what sharp does with a
                        two-descriptor GIF whose second frame cannot decode
                        (v5's frame counter counts 1 and encodes the first).
* `anim-corrupt2.webp`  (P4.112) the committed `anim-2frame.webp` with the
                        VP8 start code (`9d 01 2a`) of the SECOND `ANMF`'s
                        bitstream zeroed — the container stays well formed
                        (two `ANMF` chunks), the second frame cannot decode.
                        Derived from the committed bytes, so it IS
                        deterministic and `--check` covers it.

The GIF LZW stream is deliberately "uncompressed": a clear code precedes every
pixel, so no dictionary entry is ever added and the code width stays at 3 bits
for the 4-colour table. Every decoder accepts it; no compressor is needed.
"""

import os
import struct
import subprocess
import sys
import tempfile
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))

# A 4-colour palette: red, blue, green, white.
PALETTE = [(220, 30, 30), (30, 60, 220), (30, 180, 60), (250, 250, 250)]


# --------------------------------------------------------------------- GIF


def _lzw_uncompressed(indices, min_code_size=2):
    """A valid GIF LZW stream with a clear code before every pixel."""
    clear = 1 << min_code_size
    eoi = clear + 1
    width = min_code_size + 1
    codes = []
    for px in indices:
        codes.append(clear)
        codes.append(px)
    codes.append(eoi)
    out = bytearray()
    acc = 0
    nbits = 0
    for c in codes:
        acc |= c << nbits
        nbits += width
        while nbits >= 8:
            out.append(acc & 0xFF)
            acc >>= 8
            nbits -= 8
    if nbits:
        out.append(acc & 0xFF)
    return bytes(out)


def _lzw_invalid_code(min_code_size=2):
    """A corrupt LZW stream: a clear code, then code 7 — outside every
    dictionary a 4-colour table can hold right after a clear (the next free
    entry is 6) — then end-of-information."""
    clear = 1 << min_code_size
    width = min_code_size + 1
    acc = 0
    nbits = 0
    for c in (clear, 7, clear + 1):
        acc |= c << nbits
        nbits += width
    return acc.to_bytes((nbits + 7) // 8, "little")


def _sub_blocks(data):
    out = bytearray()
    for i in range(0, len(data), 255):
        chunk = data[i : i + 255]
        out.append(len(chunk))
        out += chunk
    out.append(0)
    return bytes(out)


def _gif(width, height, frames, loop=False, comment=None):
    out = bytearray(b"GIF89a")
    # Logical screen: global colour table present, 2 bits/entry (4 colours).
    out += struct.pack("<HHBBB", width, height, 0b1000_0001, 0, 0)
    for r, g, b in PALETTE:
        out += bytes((r, g, b))
    if loop:
        out += b"\x21\xff\x0bNETSCAPE2.0\x03\x01" + struct.pack("<H", 0) + b"\x00"
    if comment is not None:
        out += b"\x21\xfe" + _sub_blocks(comment)
    for indices in frames:
        if loop:
            # Graphic Control Extension: 100 cs delay, no transparency.
            out += b"\x21\xf9\x04\x00" + struct.pack("<H", 100) + b"\x00\x00"
        out += b"\x2c" + struct.pack("<HHHHB", 0, 0, width, height, 0)
        # A frame given as `bytes` is a ready-made (possibly corrupt) LZW
        # stream; a list is pixel indices.
        lzw = indices if isinstance(indices, bytes) else _lzw_uncompressed(indices)
        out += b"\x02" + _sub_blocks(lzw)
    out += b"\x3b"
    return bytes(out)


def _solid(width, height, index):
    return [index] * (width * height)


def _stripes(width, height):
    # Horizontal bands of 4 rows each, cycling the palette.
    return [(y // 4) % 4 for y in range(height) for _x in range(width)]


# --------------------------------------------------------------------- PNG


def _chunk(kind, data):
    return (
        struct.pack(">I", len(data))
        + kind
        + data
        + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    )


def _png_idat_payload(width, height, rgb):
    raw = bytearray()
    for _y in range(height):
        raw.append(0)  # filter: none
        for _x in range(width):
            raw += bytes(rgb)
    return zlib.compress(bytes(raw), 9)


def _png(width, height, rgb):
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + _chunk(b"IHDR", ihdr)
        + _chunk(b"IDAT", _png_idat_payload(width, height, rgb))
        + _chunk(b"IEND", b"")
    )


def _apng(width, height, colours):
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    out = bytearray(b"\x89PNG\r\n\x1a\n" + _chunk(b"IHDR", ihdr))
    out += _chunk(b"acTL", struct.pack(">II", len(colours), 0))
    seq = 0
    for i, rgb in enumerate(colours):
        # fcTL: seq, w, h, x, y, delay 1/1 s, dispose none, blend source.
        out += _chunk(
            b"fcTL", struct.pack(">IIIIIHHBB", seq, width, height, 0, 0, 1, 1, 0, 0)
        )
        seq += 1
        payload = _png_idat_payload(width, height, rgb)
        if i == 0:
            out += _chunk(b"IDAT", payload)
        else:
            out += _chunk(b"fdAT", struct.pack(">I", seq) + payload)
            seq += 1
    out += _chunk(b"IEND", b"")
    return bytes(out)


# -------------------------------------------------------------------- WebP


def _riff_chunk(kind, data):
    pad = b"\x00" if len(data) % 2 else b""
    return kind + struct.pack("<I", len(data)) + data + pad


def _u24(n):
    return struct.pack("<I", n)[:3]


def _one_anmf_webp(still_webp, width, height):
    """Wrap a plain VP8 WebP's bitstream chunk in VP8X(anim) + ANIM + ONE ANMF."""
    assert still_webp[:4] == b"RIFF" and still_webp[8:12] == b"WEBP"
    inner = still_webp[12:]
    assert inner[:4] == b"VP8 ", "cwebp -lossy must emit a simple VP8 file"
    vp8x = bytes([0b0000_0010, 0, 0, 0]) + _u24(width - 1) + _u24(height - 1)
    anim = struct.pack("<IH", 0xFFFFFFFF, 0)  # background colour, loop forever
    anmf = _u24(0) + _u24(0) + _u24(width - 1) + _u24(height - 1) + _u24(100) + b"\x00"
    anmf += inner
    body = b"WEBP" + _riff_chunk(b"VP8X", vp8x) + _riff_chunk(b"ANIM", anim)
    body += _riff_chunk(b"ANMF", anmf)
    return b"RIFF" + struct.pack("<I", len(body)) + body


def _corrupt_second_anmf(anim_webp):
    """Zero the VP8 start code of the SECOND `ANMF`'s bitstream."""
    data = bytearray(anim_webp)
    assert data[:4] == b"RIFF" and data[8:12] == b"WEBP"
    i, seen = 12, 0
    while i < len(data):
        kind = bytes(data[i : i + 4])
        size = struct.unpack("<I", data[i + 4 : i + 8])[0]
        if kind == b"ANMF":
            seen += 1
            if seen == 2:
                # ANMF payload: 16 bytes of frame header, then the `VP8 `
                # chunk (8-byte header); the start code sits 3 bytes into
                # the VP8 payload.
                vp8 = i + 8 + 16
                assert data[vp8 : vp8 + 4] == b"VP8 ", "a lossy ANMF frame"
                start = vp8 + 8 + 3
                assert data[start : start + 3] == b"\x9d\x01\x2a", "the VP8 start code"
                data[start : start + 3] = b"\x00\x00\x00"
                return bytes(data)
        i += 8 + size + (size & 1)
    raise AssertionError("no second ANMF")


# -------------------------------------------------------------------- main

RED, BLUE = PALETTE[0], PALETTE[1]


def deterministic():
    return {
        "anim-2frame.gif": _gif(32, 24, [_solid(32, 24, 0), _solid(32, 24, 1)], loop=True),
        "still-large.gif": _gif(64, 48, [_stripes(64, 48)]),
        "still-with-commas.gif": _gif(32, 24, [_solid(32, 24, 2)], comment=b"," * 200),
        "anim-2frame.apng": _apng(32, 24, [RED, BLUE]),
        # P4.112 — the corrupt-second-frame measurement.
        "anim-corrupt2.gif": _gif(
            32, 24, [_solid(32, 24, 0), _lzw_invalid_code()], loop=True
        ),
        "anim-corrupt2.webp": _corrupt_second_anmf(
            open(os.path.join(HERE, "anim-2frame.webp"), "rb").read()
        ),
    }


def build_webps():
    with tempfile.TemporaryDirectory() as tmp:
        f1 = os.path.join(tmp, "f1.png")
        f2 = os.path.join(tmp, "f2.png")
        with open(f1, "wb") as fh:
            fh.write(_png(32, 24, RED))
        with open(f2, "wb") as fh:
            fh.write(_png(32, 24, BLUE))
        anim = os.path.join(HERE, "anim-2frame.webp")
        subprocess.run(
            ["img2webp", "-loop", "0", "-d", "100", "-lossy", f1, f2, "-o", anim],
            check=True,
        )
        still = os.path.join(tmp, "f1.webp")
        subprocess.run(["cwebp", "-quiet", "-q", "80", f1, "-o", still], check=True)
        with open(still, "rb") as fh:
            one = _one_anmf_webp(fh.read(), 32, 24)
        with open(os.path.join(HERE, "one-anmf.webp"), "wb") as fh:
            fh.write(one)


def main(argv):
    files = deterministic()
    if "--check" in argv:
        bad = []
        for name, data in files.items():
            with open(os.path.join(HERE, name), "rb") as fh:
                if fh.read() != data:
                    bad.append(name)
        if bad:
            print("DIFFERS: " + ", ".join(bad))
            return 1
        print(f"OK: {len(files)} deterministic fixtures match byte-for-byte")
        return 0
    for name, data in files.items():
        with open(os.path.join(HERE, name), "wb") as fh:
            fh.write(data)
        print(f"wrote {name} ({len(data)} bytes)")
    if "--webp" in argv:
        build_webps()
        print("wrote anim-2frame.webp, one-anmf.webp (libwebp-version-dependent)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
