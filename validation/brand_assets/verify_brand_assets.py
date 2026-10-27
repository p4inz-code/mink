#!/usr/bin/env python3
"""Structural validation of the MINK brand asset tree (assets/brand).

Checks, with no third-party dependencies:

  * inventory    - the curated production files exist, and the classes of artwork that
                   are deliberately not integrated (rejected app-icon alternatives,
                   website favicons, redundant per-directory duplicates) stay out;
  * SVG          - every file is valid XML, is an <svg> root with width/height/viewBox,
                   has no external reference, and every internal reference
                   (``url(#id)``/``href="#id"``) resolves to an id in the same document;
  * PNG          - signature, IHDR, chunk CRCs, IDAT stream integrity, RGBA colour type,
                   and that the file name's size suffix matches the real dimensions;
  * ICO          - the two Windows icons contain exactly the expected 7 frames
                   (16/24/32/48/64/128/256), each frame is a valid PNG of the matching
                   size, and each frame is byte-identical to the shipped PNG frame it was
                   built from;
  * kit copies   - the ICOs shipped in npm/mink/windows are byte-identical to the
                   authoritative copies in assets/brand/windows/ico, and the npm package
                   still publishes that directory;
  * duplicates   - no two files in the tree are byte-identical (each artwork is stored
                   once, in the directory that owns it).

Usage:  python validation/brand_assets/verify_brand_assets.py
Exit code 0 when every check passes, 1 otherwise.
"""

from __future__ import annotations

import json
import struct
import sys
import xml.etree.ElementTree as ET
import zlib
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
BRAND = REPO / "assets" / "brand"
KIT = REPO / "npm" / "mink" / "windows"

SVG_NS = "http://www.w3.org/2000/svg"
XLINK_NS = "http://www.w3.org/1999/xlink"
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]

failures: list[str] = []
checks: list[tuple[str, str]] = []


def check(label: str, ok: bool, detail: str = "") -> None:
    checks.append((label, "PASS" if ok else "FAIL"))
    if not ok:
        failures.append(f"{label}: {detail}" if detail else label)


def info(label: str, detail: str) -> None:
    checks.append((label, f"INFO ({detail})"))


# --- required / forbidden inventory ----------------------------------------

REQUIRED = [
    "masters/svg/mink-master.svg",
    "masters/vector/mink-geometry.json",
    "icon/application/mink-app-master.svg",
    "icon/source-file/mink-file-master.svg",
    "icon/source-file/mink-file-mono.svg",
    "windows/ico/mink-app.ico",
    "windows/ico/mink-file.ico",
    "web/github/mink-readme-header-light.png",
    "web/github/mink-readme-header-dark.png",
    "on-background/mink-logo-primary-on-dark.svg",
    "on-background/mink-logo-primary-on-light.svg",
    "on-background/mink-logo-primary-single-color.svg",
    "README.md",
]
REQUIRED += [f"logo/{lockup}/mink-logo-{lockup}-{colour}.svg"
             for lockup in ("primary", "horizontal", "compact")
             for colour in ("black", "ink", "white")]
REQUIRED += [f"logo/wordmark/mink-wordmark-{c}.svg" for c in ("black", "ink", "white")]
REQUIRED += [f"logo/symbol/mink-symbol-{c}.svg" for c in ("black", "ink", "white")]
REQUIRED += [f"logo/symbol/mink-symbol-{variant}-{c}.svg"
             for variant in ("optical-small", "silhouette")
             for c in ("black", "ink", "white")]
REQUIRED += [f"icon/application/png/mink-app-{s}.png" for s in (16, 24, 32, 48, 64, 128, 256)]
REQUIRED += [f"icon/source-file/png/mink-file-{s}.png" for s in (16, 24, 32, 48, 64, 128, 256, 1024)]
REQUIRED += [f"logo/symbol/png/mink-symbol-{c}-{s}.png"
             for c in ("black", "ink", "white") for s in (16, 32, 64, 128, 256, 512)]
REQUIRED += [f"previews/explorer/explorer-{theme}.png" for theme in ("light", "dark")]
REQUIRED += ["windows/ico/mink-app.ico", "windows/ico/mink-file.ico"]
REQUIRED += ["web/social/mink-social-preview-dark-1280x640.png",
             "web/social/mink-profile-avatar-1000.png"]

missing = [p for p in REQUIRED if not (BRAND / p).is_file()]
check(f"inventory ({len(REQUIRED)} required files)", not missing, f"missing {missing}")

FORBIDDEN = [
    "README.md.bak",
    "extras",                     # rejected app-icon alternatives
    "web/favicon",                # belongs to a website project, not this repository
    "logo/monochrome",            # byte-identical copies of the colourway files
    "windows/png",                # byte-identical copies of icon/*/png
    "icon/application/mink-app-256.svg",   # byte-identical to mink-app-master.svg
]
present = [p for p in FORBIDDEN if (BRAND / p).exists()]
check("no rejected or duplicate artwork", not present, f"unexpected {present}")

svgs = sorted(BRAND.rglob("*.svg"))
pngs = sorted(BRAND.rglob("*.png"))
# The curated set is 49 SVG + 63 PNG; nothing may disappear from it unnoticed.
check("asset volume", len(svgs) >= 49 and len(pngs) >= 63,
      f"{len(svgs)} SVG, {len(pngs)} PNG - below the curated 49 SVG / 63 PNG")

# --- SVG --------------------------------------------------------------------

svg_errors: list[str] = []
for path in svgs:
    rel = path.relative_to(BRAND).as_posix()
    try:
        root = ET.parse(path).getroot()
    except ET.ParseError as exc:
        svg_errors.append(f"{rel}: invalid XML ({exc})")
        continue
    if root.tag != f"{{{SVG_NS}}}svg":
        svg_errors.append(f"{rel}: root element is {root.tag!r}, not svg")
        continue
    if root.get("viewBox") is None:
        svg_errors.append(f"{rel}: no viewBox")
    for attr in ("width", "height"):
        if root.get(attr) is None:
            svg_errors.append(f"{rel}: no {attr}")

    ids = {el.get("id") for el in root.iter() if el.get("id")}
    for el in root.iter():
        if el.tag == f"{{{SVG_NS}}}script":
            svg_errors.append(f"{rel}: contains a script element")
        if el.tag == f"{{{SVG_NS}}}image":
            svg_errors.append(f"{rel}: contains an image element (external resource)")
        for attr in ("href", f"{{{XLINK_NS}}}href"):
            value = el.get(attr)
            if value and not value.startswith("#"):
                svg_errors.append(f"{rel}: external reference {attr}={value!r}")
            elif value and value[1:] not in ids:
                svg_errors.append(f"{rel}: broken reference {value!r}")

    for raw in (root.attrib.get("style", ""),):
        if "url(" in raw and not raw.startswith("url(#"):
            svg_errors.append(f"{rel}: external style reference {raw!r}")
    for el in root.iter():
        for raw in (el.get("style") or "", el.get("fill") or ""):
            if "url(" in raw and "url(#" not in raw:
                svg_errors.append(f"{rel}: external paint reference {raw!r}")

check(f"SVG structure ({len(svgs)} files)", not svg_errors,
      "; ".join(svg_errors[:5]) + (" ..." if len(svg_errors) > 5 else ""))

# --- PNG --------------------------------------------------------------------

PNG_MAGIC = b"\x89PNG\r\n\x1a\n"


def read_png(path: Path) -> tuple[int, int, int, int]:
    """Return (width, height, bit depth, colour type) after verifying the stream."""
    data = path.read_bytes()
    if data[:8] != PNG_MAGIC:
        raise ValueError("bad signature")
    pos = 8
    width = height = depth = colour = -1
    idat = bytearray()
    seen_ihdr = seen_iend = False
    while pos + 8 <= len(data):
        length, kind = struct.unpack_from(">I4s", data, pos)
        body = data[pos + 8:pos + 8 + length]
        crc = struct.unpack_from(">I", data, pos + 8 + length)[0]
        if zlib.crc32(kind + body) & 0xFFFFFFFF != crc:
            raise ValueError(f"CRC mismatch in {kind.decode()} chunk")
        if kind == b"IHDR":
            width, height, depth, colour = struct.unpack_from(">IIBB", body, 0)
            seen_ihdr = True
        elif kind == b"IDAT":
            idat += body
        elif kind == b"IEND":
            seen_iend = True
        pos += 12 + length
    if not seen_ihdr:
        raise ValueError("no IHDR")
    if not seen_iend:
        raise ValueError("no IEND")
    raw = zlib.decompress(bytes(idat))
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[colour]
    expected = height * (1 + width * channels * (depth // 8))
    if len(raw) != expected:
        raise ValueError(f"IDAT decodes to {len(raw)} bytes, expected {expected}")
    return width, height, depth, colour


def declared_size(path: Path) -> tuple[int, int] | None:
    """Parse the size encoded in the file name: '-256' or '-1280x640'."""
    stem = path.stem
    if stem.endswith("-2x"):
        return None
    if "x" in stem.rsplit("-", 1)[-1]:
        tail = stem.rsplit("-", 1)[-1]
        w, _, h = tail.partition("x")
        if w.isdigit() and h.isdigit():
            return int(w), int(h)
    tail = stem.rsplit("-", 1)[-1]
    if tail.isdigit():
        return int(tail), int(tail)
    return None


png_errors: list[str] = []
for path in pngs:
    rel = path.relative_to(BRAND).as_posix()
    try:
        width, height, _depth, colour = read_png(path)
    except (ValueError, OSError) as exc:
        png_errors.append(f"{rel}: {exc}")
        continue
    if colour not in (4, 6):
        png_errors.append(f"{rel}: colour type {colour} has no alpha channel")
    declared = declared_size(path)
    if declared:
        want_w, want_h = declared
        square = want_w == want_h
        if square and (width != want_w or height != want_h) and want_w not in (width, height):
            png_errors.append(f"{rel}: {width}x{height} does not carry the declared {want_w}")
        if not square and (width, height) != (want_w, want_h):
            png_errors.append(f"{rel}: {width}x{height} does not match the declared name")

check(f"PNG integrity and sizes ({len(pngs)} files)", not png_errors,
      "; ".join(png_errors[:5]) + (" ..." if len(png_errors) > 5 else ""))

# --- ICO --------------------------------------------------------------------


def ico_frames(path: Path) -> list[tuple[int, bytes]]:
    data = path.read_bytes()
    reserved, kind, count = struct.unpack_from("<HHH", data, 0)
    if (reserved, kind) != (0, 1):
        raise ValueError("not an icon file (reserved/type header)")
    frames: list[tuple[int, bytes]] = []
    for index in range(count):
        width, height, _colours, _res, _planes, bpp, length, offset = struct.unpack_from(
            "<BBBBHHII", data, 6 + 16 * index)
        width = width or 256
        height = height or 256
        if bpp not in (32,):
            raise ValueError(f"frame {index} is {bpp}bpp, expected 32bpp")
        frame = data[offset:offset + length]
        if frame[:8] != PNG_MAGIC:
            raise ValueError(f"frame {index} is not a PNG frame")
        frames.append((width, frame))
    return frames


ico_errors: list[str] = []
for name, pngdir in (("mink-app.ico", "icon/application/png"),
                     ("mink-file.ico", "icon/source-file/png")):
    rel = f"windows/ico/{name}"
    path = BRAND / rel
    try:
        frames = ico_frames(path)
    except (ValueError, OSError) as exc:
        ico_errors.append(f"{rel}: {exc}")
        continue
    sizes = [size for size, _ in frames]
    if sizes != ICO_SIZES:
        ico_errors.append(f"{rel}: frames {sizes}, expected {ICO_SIZES}")
        continue
    for size, frame in frames:
        export = BRAND / pngdir / f"{Path(name).stem}-{size}.png"
        if not export.is_file():
            ico_errors.append(f"{rel}: no PNG export for the {size}px frame")
            continue
        if export.read_bytes() != frame:
            ico_errors.append(f"{rel}: the {size}px frame differs from {export.name}")
        else:
            try:
                width, height, _, _ = read_png(export)
            except ValueError as exc:
                ico_errors.append(f"{rel}: {size}px frame is not a valid PNG ({exc})")
                continue
            if (width, height) != (size, size):
                ico_errors.append(f"{rel}: the {size}px frame is {width}x{height}")

check("ICO structure and frame consistency (2 files)", not ico_errors,
      "; ".join(ico_errors[:5]))

# --- shipped kit copies ------------------------------------------------------

kit_errors: list[str] = []
for name in ("mink-app.ico", "mink-file.ico"):
    brand_copy = BRAND / "windows" / "ico" / name
    kit_copy = KIT / name
    if not kit_copy.is_file():
        kit_errors.append(f"npm/mink/windows/{name} is missing")
    elif kit_copy.read_bytes() != brand_copy.read_bytes():
        kit_errors.append(f"npm/mink/windows/{name} differs from assets/brand/windows/ico/{name}")
for name in ("register-mink-filetype.ps1", "mink.rc", "README.md"):
    if not (KIT / name).is_file():
        kit_errors.append(f"npm/mink/windows/{name} is missing")
package_json = KIT.parent / "package.json"
try:
    published = json.loads(package_json.read_text(encoding="utf-8"))
    if "windows" not in published.get("files", []):
        kit_errors.append('npm/mink/package.json does not list "windows" in files[]')
except (OSError, ValueError) as exc:
    kit_errors.append(f"npm/mink/package.json could not be read ({exc})")
check("shipped Windows kit", not kit_errors, "; ".join(kit_errors))

# --- duplicate content ------------------------------------------------------

digests: dict[bytes, str] = {}
duplicates: list[str] = []
for path in sorted(p for p in BRAND.rglob("*") if p.is_file()):
    digest = zlib.crc32(path.read_bytes()).to_bytes(4, "big") + struct.pack(">I", len(path.read_bytes()))
    rel = path.relative_to(BRAND).as_posix()
    if digest in digests:
        duplicates.append(f"{rel} == {digests[digest]}")
    else:
        digests[digest] = rel
check("no byte-identical duplicates", not duplicates, "; ".join(duplicates[:5]))

# --- mink.exe icon resource (informational) ---------------------------------


def pe_resource_status(path: Path) -> str:
    data = path.read_bytes()
    if data[:2] != b"MZ":
        return "not a PE file"
    pe_offset = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe_offset:pe_offset + 4] != b"PE\0\0":
        return "no PE signature"
    optional = pe_offset + 4 + 20
    magic = struct.unpack_from("<H", data, optional)[0]
    data_dirs = optional + (96 if magic == 0x10B else 112)
    rva, size = struct.unpack_from("<II", data, data_dirs + 2 * 8)
    if rva == 0 and size == 0:
        return "no resource directory (no embedded icon)"
    return f"resource directory present (rva {rva}, {size} bytes)"


exe = REPO / "npm" / "mink" / "bin" / "mink.exe"
if exe.is_file():
    info("mink.exe icon resource", pe_resource_status(exe))
else:
    info("mink.exe icon resource", "npm/mink/bin/mink.exe not present in this checkout")

# --- report -----------------------------------------------------------------

width = max(len(label) for label, _ in checks)
print("MINK brand assets - validation")
for label, result in checks:
    print(f"  {label.ljust(width)} : {result}")
print()
if failures:
    print(f"RESULT: FAIL - {len(failures)} check(s) failed")
    for failure in failures:
        print(f"  - {failure}")
    sys.exit(1)
print(f"RESULT: PASS - {len(checks)} checks")
