#!/usr/bin/env python3
"""Read a PE image's import table without objdump/dumpbin.

    python benchmarks/m4/harness/pe_imports.py <exe> [<exe> ...]

`objdump` and `dumpbin` are not installed on the M4 host, so this small
read-only parser supplies the artifact-dependency evidence required by the
build/distribution panel. It reports imported DLL names only; it does not
execute, modify or trust the image.
"""
import struct
import sys


def _rva_to_offset(sections, rva):
    for virtual_address, virtual_size, raw_pointer, raw_size in sections:
        size = max(virtual_size, raw_size)
        if virtual_address <= rva < virtual_address + size:
            return raw_pointer + (rva - virtual_address)
    raise ValueError("RVA 0x%x is not inside any section" % rva)


def _cstring(data, offset):
    end = data.find(b"\0", offset)
    if end < 0:
        return ""
    return data[offset:end].decode("ascii", "replace")


def imports(path):
    with open(path, "rb") as handle:
        data = handle.read()

    if data[:2] != b"MZ":
        raise ValueError("%s is not a PE image" % path)
    pe_offset = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe_offset:pe_offset + 4] != b"PE\0\0":
        raise ValueError("%s has no PE signature" % path)

    coff = pe_offset + 4
    section_count = struct.unpack_from("<H", data, coff + 2)[0]
    optional_size = struct.unpack_from("<H", data, coff + 16)[0]
    optional = coff + 20
    magic = struct.unpack_from("<H", data, optional)[0]
    if magic == 0x20B:
        directories = optional + 112
        image_base = struct.unpack_from("<Q", data, optional + 24)[0]
    elif magic == 0x10B:
        directories = optional + 96
        image_base = struct.unpack_from("<I", data, optional + 28)[0]
    else:
        raise ValueError("%s has an unknown optional-header magic" % path)

    # Data directory entry 1 is the import table.
    import_rva, import_size = struct.unpack_from("<II", data, directories + 8)
    section_table = optional + optional_size
    sections = []
    for index in range(section_count):
        base = section_table + index * 40
        virtual_size, virtual_address = struct.unpack_from("<II", data, base + 8)
        raw_size, raw_pointer = struct.unpack_from("<II", data, base + 16)
        sections.append((virtual_address, virtual_size, raw_pointer, raw_size))

    names = []
    if import_rva and import_size:
        offset = _rva_to_offset(sections, import_rva)
        while True:
            entry = data[offset:offset + 20]
            if len(entry) < 20 or entry == b"\0" * 20:
                break
            name_rva = struct.unpack_from("<I", entry, 12)[0]
            if name_rva:
                names.append(_cstring(data, _rva_to_offset(sections, name_rva)))
            offset += 20
    return image_base, names


def main(argv):
    if not argv:
        print(__doc__.strip())
        return 2
    for path in argv:
        try:
            _, names = imports(path)
        except (OSError, ValueError, struct.error) as error:
            print("%s: %s" % (path, error))
            continue
        print(path)
        for name in names:
            print("  " + name)
        if not names:
            print("  (no imports)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
