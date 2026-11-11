#!/usr/bin/env python3
"""Embed RT_MANIFEST + icon resources into a PE32+ executable.

Replacement for mt.exe / rc.exe when only a minimal MSVC toolchain is
available (the host endpoint protection blocks build scripts from spawning
cl.exe / rc.exe / mt.exe). The manifest enables the Microsoft.Windows
Common-Controls v6 activation context, which is required for
TaskDialogIndirect; the icon resources supply Tauri's default window icon.

Usage:
    python embed-manifest.py <exe> <manifest.xml> [--icon <icon.ico>] [--check]

The exe is modified in place. A pristine copy is saved as <exe>.bak on the
first run and restored before every subsequent run, so the operation is
idempotent.
"""
import os
import shutil
import struct
import sys

RT_ICON = 3
RT_GROUP_ICON = 14
RT_MANIFEST = 24
LANG_EN_US = 1033
APP_ICON_RESOURCE_ID = 32512  # tauri-utils::platform::WINDOWS_APP_ICON_RESOURCE_ID
SCN_CNT_INITIALIZED_DATA = 0x00000040
SCN_MEM_READ = 0x40000000


def align(value, alignment):
    return (value + alignment - 1) // alignment * alignment


def parse_icon(ico_bytes):
    """Split an .ico into (RT_ICON blobs, RT_GROUP_ICON blob)."""
    reserved, kind, count = struct.unpack_from("<HHH", ico_bytes, 0)
    if reserved != 0 or kind != 1 or count < 1:
        raise SystemExit("invalid .ico file")
    entries = []
    blobs = []
    for i in range(count):
        o = 6 + 16 * i
        w, h, colors, res, planes, bits, size, offset = struct.unpack_from(
            "<BBBBHHII", ico_bytes, o
        )
        blobs.append(ico_bytes[offset:offset + size])
        entries.append((w, h, colors, res, planes, bits, size, i + 1))
    group = bytearray(struct.pack("<HHH", 0, 1, count))
    for w, h, colors, res, planes, bits, size, rid in entries:
        group.extend(struct.pack("<BBBBHHIH", w, h, colors, res, planes, bits, size, rid))
    return list(zip([i + 1 for i in range(count)], blobs)), bytes(group)


def build_resource_blob(resources, section_rva):
    """Build a PE resource tree.

    resources: list of (type_id, resource_id, lang_id, payload_bytes)
    """
    types = sorted({r[0] for r in resources})
    ids_by_type = {
        t: sorted({r[1] for r in resources if r[0] == t}) for t in types
    }
    langs_by_key = {}
    for r in resources:
        langs_by_key.setdefault((r[0], r[1]), []).append(r[2])
    for k in langs_by_key:
        langs_by_key[k] = sorted(set(langs_by_key[k]))

    off = 0
    root_off = off
    off += 16 + 8 * len(types)

    type_off = {}
    for t in types:
        type_off[t] = off
        off += 16 + 8 * len(ids_by_type[t])

    lang_off = {}
    for t in types:
        for rid in ids_by_type[t]:
            lang_off[(t, rid)] = off
            off += 16 + 8 * len(langs_by_key[(t, rid)])

    data_entry_off = {}
    for r in resources:
        data_entry_off[(r[0], r[1], r[2])] = off
        off += 16

    off = align(off, 8)
    blob_off = {}
    for r in resources:
        blob_off[(r[0], r[1], r[2])] = off
        off = align(off + len(r[3]), 8)
    total = off
    buf = bytearray(total)

    def directory(at, count):
        struct.pack_into("<IIHHHH", buf, at, 0, 0, 0, 0, 0, count)

    def entry(at, name, target, is_subdir):
        struct.pack_into("<II", buf, at, name, target | (0x80000000 if is_subdir else 0))

    directory(root_off, len(types))
    for i, t in enumerate(types):
        entry(root_off + 16 + 8 * i, t, type_off[t], True)

    for t in types:
        at = type_off[t]
        directory(at, len(ids_by_type[t]))
        for i, rid in enumerate(ids_by_type[t]):
            entry(at + 16 + 8 * i, rid, lang_off[(t, rid)], True)

    for (t, rid), at in lang_off.items():
        directory(at, len(langs_by_key[(t, rid)]))
        for i, lang in enumerate(langs_by_key[(t, rid)]):
            entry(at + 16 + 8 * i, lang, data_entry_off[(t, rid, lang)], False)

    for r in resources:
        t, rid, lang, payload = r
        at = data_entry_off[(t, rid, lang)]
        struct.pack_into(
            "<IIII", buf, at,
            section_rva + blob_off[(t, rid, lang)], len(payload), 0, 0,
        )
        buf[blob_off[(t, rid, lang)]:blob_off[(t, rid, lang)] + len(payload)] = payload

    return bytes(buf)


def read_resource_dir(exe_path):
    with open(exe_path, "rb") as f:
        data = f.read()
    if data[:2] != b"MZ":
        return None, "not a PE file"
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        return None, "bad PE signature"
    opt = pe + 4 + 20
    if struct.unpack_from("<H", data, opt)[0] != 0x20B:
        return None, "not PE32+"
    rva, size = struct.unpack_from("<II", data, opt + 112 + 2 * 8)
    return (rva, size), "resource directory RVA=0x%X size=%d" % (rva, size)


def embed(exe_path, manifest_path, icon_path=None):
    with open(manifest_path, "rb") as f:
        xml = f.read()
    if xml.startswith(b"\xef\xbb\xbf"):
        xml = xml[3:]
    if b"Microsoft.Windows.Common-Controls" not in xml:
        raise SystemExit("manifest does not declare Common-Controls v6")

    resources = [(RT_MANIFEST, 1, LANG_EN_US, xml)]
    if icon_path:
        with open(icon_path, "rb") as f:
            icon_blobs, group_blob = parse_icon(f.read())
        for rid, blob in icon_blobs:
            resources.append((RT_ICON, rid, LANG_EN_US, blob))
        resources.append((RT_GROUP_ICON, APP_ICON_RESOURCE_ID, LANG_EN_US, group_blob))

    # Restore the pristine binary so repeated runs stay idempotent.
    bak = exe_path + ".bak"
    if os.path.exists(bak):
        shutil.copyfile(bak, exe_path)
    else:
        shutil.copyfile(exe_path, bak)

    with open(exe_path, "rb") as f:
        data = bytearray(f.read())

    if data[:2] != b"MZ":
        raise SystemExit("not a PE file: %s" % exe_path)
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise SystemExit("bad PE signature")
    coff = pe + 4
    nsec = struct.unpack_from("<H", data, coff + 2)[0]
    size_opt = struct.unpack_from("<H", data, coff + 16)[0]
    opt = coff + 20
    if struct.unpack_from("<H", data, opt)[0] != 0x20B:
        raise SystemExit("only PE32+ (x64) is supported")
    sec_align = struct.unpack_from("<I", data, opt + 32)[0]
    file_align = struct.unpack_from("<I", data, opt + 36)[0]
    size_headers = struct.unpack_from("<I", data, opt + 60)[0]
    num_dd = struct.unpack_from("<I", data, opt + 108)[0]
    if num_dd < 3:
        raise SystemExit("optional header has no resource data directory")
    rsrc_dir_off = opt + 112 + 2 * 8
    rsrc_rva, rsrc_size = struct.unpack_from("<II", data, rsrc_dir_off)
    if rsrc_rva or rsrc_size:
        raise SystemExit("base exe already has a resource directory; restore a clean build")

    sectab = opt + size_opt
    if sectab + (nsec + 1) * 40 > size_headers:
        raise SystemExit("no room for an extra section header (SizeOfHeaders=%d)" % size_headers)

    end_va = 0
    for i in range(nsec):
        s = sectab + i * 40
        va = struct.unpack_from("<I", data, s + 12)[0]
        vsize = struct.unpack_from("<I", data, s + 8)[0]
        rawsize = struct.unpack_from("<I", data, s + 16)[0]
        end_va = max(end_va, va + max(vsize, rawsize))
    new_va = align(end_va, sec_align)

    blob = build_resource_blob(resources, new_va)
    raw_size = align(len(blob), file_align)
    raw_ptr = align(len(data), file_align)

    if num_dd > 4:
        cert_off, cert_size = struct.unpack_from("<II", data, opt + 112 + 4 * 8)
        if cert_off or cert_size:
            raise SystemExit("exe has a certificate table / overlay; cannot append a section")

    new_sec = sectab + nsec * 40
    struct.pack_into("<8s", data, new_sec, b".rsrc\0\0\0")
    struct.pack_into("<IIII", data, new_sec + 8,
                     len(blob), new_va, raw_size, raw_ptr)
    struct.pack_into("<IIHH", data, new_sec + 24, 0, 0, 0, 0)
    struct.pack_into("<I", data, new_sec + 36,
                     SCN_CNT_INITIALIZED_DATA | SCN_MEM_READ)

    struct.pack_into("<H", data, coff + 2, nsec + 1)
    struct.pack_into("<I", data, opt + 56, align(new_va + len(blob), sec_align))
    struct.pack_into("<II", data, rsrc_dir_off, new_va, len(blob))
    struct.pack_into("<I", data, opt + 64, 0)  # checksum not verified for user-mode exe

    if len(data) < raw_ptr + raw_size:
        data.extend(b"\0" * (raw_ptr + raw_size - len(data)))
    data[raw_ptr:raw_ptr + len(blob)] = blob

    with open(exe_path, "wb") as f:
        f.write(data)
    print("embedded %d resources into .rsrc: RVA=0x%X raw=0x%X size=%d"
          % (len(resources), new_va, raw_ptr, raw_size))
    print("  RT_MANIFEST(24)/1 (%d bytes)" % len(xml))
    if icon_path:
        print("  RT_ICON(3)/1..%d, RT_GROUP_ICON(14)/%d"
              % (len(icon_blobs), APP_ICON_RESOURCE_ID))
    print("backup: %s" % bak)


if __name__ == "__main__":
    args = sys.argv[1:]
    if len(args) < 2:
        raise SystemExit(__doc__)
    exe, manifest = args[0], args[1]
    if "--check" in args:
        present, detail = read_resource_dir(exe)
        print(("HAS" if present and present[0] else "NO") + " resource: " + detail)
        sys.exit(0)
    icon = None
    if "--icon" in args:
        icon = args[args.index("--icon") + 1]
    embed(exe, manifest, icon)
