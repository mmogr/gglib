#!/usr/bin/env python3
"""A Windows program gglib ships needs no Visual C++ runtime installed.

Usage:
    ./scripts/check_pe_imports.py <program.exe> [<program.exe> ...]
    ./scripts/check_pe_imports.py --self-test

A Windows machine without the Visual C++ 2015-2022 Redistributable does not
start a program that imports `VCRUNTIME140.dll`, `VCRUNTIME140_1.dll` or
`MSVCP140.dll`: Windows refuses it with "the code execution cannot proceed"
before its first line runs (mmogr/gglib#1959, finding A1). Rust links that
runtime dynamically on `*-windows-msvc` unless told otherwise, so a plain
`cargo build` makes exactly such a program. This check reads each named
file's import table and fails when it names one of those DLLs.

What is refused, case-insensitively: every DLL the Visual C++ Redistributable
installs, whatever its version (`vcruntime*`, `msvcp*`, `msvcr<n>`,
`concrt*`, `vccorlib*`, `vcomp*`, `vcamp*`, `mfc*`), their debug builds, and
`ucrtbased.dll`, the debug C runtime only Visual Studio installs. What is
allowed: `ucrtbase.dll` and the `api-ms-win-crt-*` sets, the Universal C
Runtime, which is part of Windows 10 and 11; `msvcrt.dll`, Windows' own; and
every other system DLL. The desktop app keeps the Universal C Runtime
dynamic on purpose: tauri-build links the Visual C++ runtime statically and
`ucrt.lib` dynamically (`build > windows > staticVCRuntime`, true by
default).

Both import tables are read: the ordinary one, which Windows resolves before
the program starts, and the delay-load one, which it resolves at the first
call, since a delay-loaded runtime fails the same machine later. Each file's
full import list is printed, so a run's log says what the program needs.

A file that is not a PE image, an import entry that points outside the file,
and an image that imports nothing each fail the check: every program Rust or
MSVC links imports at least KERNEL32.dll, so for one of gglib's an empty list
is a misreading, never a pass. (A DLL can import nothing; this check is for
programs.)

What it does NOT check, so nobody reads more into a pass than is there:

* the DLLs those DLLs import, or a DLL the program loads by name at run time
  (`LoadLibrary`); the engines gglib downloads (llama-server, sd-server) are
  upstream builds that do need the runtime, which is not this check's
  business;
* that the program starts: only a Windows machine can show that.

The self-test builds PE images from bytes, 32- and 64-bit, with ordinary and
delay-load imports and sections whose file offsets differ from their
addresses, plants each forbidden form and each misreading above, and passes
when each is found and nothing else is.

Exit codes:
    0  no file imports the runtime; or the self-test passed
    1  a file imports the runtime; or the self-test failed
    2  a file could not be read as a Windows program
"""

from __future__ import annotations

import os
import re
import struct
import sys
import tempfile

# The Visual C++ Redistributable's DLLs, any version, release or debug, and
# the debug Universal C Runtime. A version number always follows the stem,
# which keeps Windows' own msvcrt.dll out.
FORBIDDEN = re.compile(
    r"^(?:(?:vcruntime|msvcp|msvcr|concrt|vccorlib|vcomp|vcamp|mfcm|mfc)\d+[a-z0-9_]*|ucrtbased)\.dll$",
    re.IGNORECASE,
)

IMPORT_DIRECTORY = 1
DELAY_IMPORT_DIRECTORY = 13


class NotAProgram(Exception):
    """The bytes are not a PE image this reader can follow."""


def imports(data: bytes) -> list[tuple[str, bool]]:
    """Each DLL the image imports, in table order, with whether it is delay-loaded."""

    def unpack(fmt: str, offset: int) -> tuple:
        if offset < 0 or offset + struct.calcsize(fmt) > len(data):
            raise NotAProgram(f"a header runs past the end of the file at offset {offset:#x}")
        return struct.unpack_from(fmt, data, offset)

    if data[:2] != b"MZ":
        raise NotAProgram("no MZ header")
    (pe,) = unpack("<I", 0x3C)
    if data[pe : pe + 4] != b"PE\0\0":
        raise NotAProgram("no PE signature")
    _, sections, _, _, _, optional_size, _ = unpack("<HHIIIHH", pe + 4)
    optional = pe + 24
    (magic,) = unpack("<H", optional)
    if magic == 0x10B:
        (image_base,) = unpack("<I", optional + 28)
        directories = optional + 96
        (directory_count,) = unpack("<I", optional + 92)
    elif magic == 0x20B:
        (image_base,) = unpack("<Q", optional + 24)
        directories = optional + 112
        (directory_count,) = unpack("<I", optional + 108)
    else:
        raise NotAProgram(f"unknown optional header magic {magic:#x}")

    table = []
    for index in range(sections):
        _, virtual_size, address, raw_size, raw_offset = unpack("<8sIIII", optional + optional_size + index * 40)
        table.append((address, max(virtual_size, raw_size), raw_offset, raw_size))

    def offset_of(rva: int) -> int:
        for address, size, raw_offset, raw_size in table:
            if address <= rva < address + size:
                if rva - address >= raw_size:
                    break
                return raw_offset + rva - address
        raise NotAProgram(f"address {rva:#x} is in no section of the file")

    def name_at(rva: int) -> str:
        start = offset_of(rva)
        end = data.find(b"\0", start)
        if end < 0:
            raise NotAProgram(f"the name at {rva:#x} has no end")
        return data[start:end].decode("ascii", errors="replace")

    def directory(index: int) -> int:
        if index >= directory_count:
            return 0
        (rva, _) = unpack("<II", directories + index * 8)
        return rva

    found: list[tuple[str, bool]] = []
    rva = directory(IMPORT_DIRECTORY)
    if rva:
        entry = offset_of(rva)
        while True:
            lookup, _, _, name, thunks = unpack("<IIIII", entry)
            if not (lookup or name or thunks):
                break
            found.append((name_at(name), False))
            entry += 20
    rva = directory(DELAY_IMPORT_DIRECTORY)
    if rva:
        entry = offset_of(rva)
        while True:
            attributes, name, handle, thunks, *_ = unpack("<8I", entry)
            if not (name or handle or thunks):
                break
            # Bit 0 clear is the old form, whose fields are addresses, not RVAs.
            # Only 32-bit images have it: a 64-bit address does not fit.
            found.append((name_at(name if attributes & 1 else name - image_base), True))
            entry += 32
    if not found:
        raise NotAProgram("it imports nothing, which no program Rust or MSVC links does")
    return found


def forbidden(found: list[tuple[str, bool]]) -> list[str]:
    return [name for name, _ in found if FORBIDDEN.match(name)]


def check(paths: list[str]) -> int:
    status = 0
    for path in paths:
        try:
            with open(path, "rb") as handle:
                found = imports(handle.read())
        except (OSError, NotAProgram) as error:
            print(f"❌ {path}: not read as a Windows program: {error}")
            status = max(status, 2)
            continue
        listed = ", ".join(name + (" (delay-loaded)" if delayed else "") for name, delayed in found)
        bad = forbidden(found)
        if bad:
            print(f"❌ {path} imports the Visual C++ runtime: {', '.join(bad)}")
            print(f"     A machine without the Visual C++ Redistributable will not start it. All its imports: {listed}")
            status = max(status, 1)
        else:
            print(f"✅ {path} imports no Visual C++ runtime. Its imports: {listed}")
    return status


# --- self-test -------------------------------------------------------------


def build_image(dlls: list[str], delayed: list[str] = (), bits: int = 64, old_delay: bool = False) -> bytes:
    """A minimal PE image whose one section holds the given import tables.

    The section's address (0x2000) differs from its file offset (0x400), so a
    reader that takes an RVA as a file offset reads the wrong bytes.
    """
    address, raw_offset, image_base = 0x2000, 0x400, 0x140000000 if bits == 64 else 0x400000
    body = bytearray()
    names: dict[str, int] = {}
    for dll in [*dlls, *delayed]:
        names[dll] = address + len(body)
        body += dll.encode("ascii") + b"\0"
    while len(body) % 4:
        body += b"\0"
    imports_rva = address + len(body) if dlls else 0
    for dll in dlls:
        body += struct.pack("<IIIII", 0, 0, 0, names[dll], 0)
    if dlls:
        body += bytes(20)
    delay_rva = address + len(body) if delayed else 0
    for dll in delayed:
        name = names[dll] + (image_base if old_delay else 0)
        body += struct.pack("<8I", 0 if old_delay else 1, name, 0, 0x1, 0, 0, 0, 0)
    if delayed:
        body += bytes(32)
    raw_size = (len(body) + 0x1FF) & ~0x1FF
    body += bytes(raw_size - len(body))

    directories = bytearray(16 * 8)
    struct.pack_into("<II", directories, IMPORT_DIRECTORY * 8, imports_rva, 0)
    struct.pack_into("<II", directories, DELAY_IMPORT_DIRECTORY * 8, delay_rva, 0)
    if bits == 64:
        optional = struct.pack("<HBBIIIIIQ", 0x20B, 14, 0, raw_size, 0, 0, 0, 0x1000, image_base)
        optional += bytes(108 - len(optional)) + struct.pack("<I", 16)
    else:
        optional = struct.pack("<HBBIIIIIII", 0x10B, 14, 0, raw_size, 0, 0, 0, 0x1000, 0, image_base)
        optional += bytes(92 - len(optional)) + struct.pack("<I", 16)
    optional += directories
    machine = 0x8664 if bits == 64 else 0x14C
    coff = struct.pack("<HHIIIHH", machine, 1, 0, 0, 0, len(optional), 0x22)
    section = struct.pack("<8sIIIIIIHHI", b".idata", len(body), address, raw_size, raw_offset, 0, 0, 0, 0, 0xC0000040)
    headers = bytearray(b"MZ" + bytes(0x3A) + struct.pack("<I", 0x80))
    headers += bytes(0x80 - len(headers)) + b"PE\0\0" + coff + optional + section
    if len(headers) > raw_offset:
        raise AssertionError("the planted headers overran the section")
    return bytes(headers) + bytes(raw_offset - len(headers)) + bytes(body)


SYSTEM = ["KERNEL32.dll", "ntdll.dll", "bcrypt.dll", "ws2_32.dll", "msvcrt.dll", "ucrtbase.dll"]
UCRT = ["api-ms-win-crt-runtime-l1-1-0.dll", "api-ms-win-crt-heap-l1-1-0.dll", "api-ms-win-crt-string-l1-1-0.dll"]

# (name, image bytes, the forbidden imports planted; None for a misreading, exit 2)
PLANTED: list[tuple[str, bytes, list[str] | None]] = [
    ("a static-CRT program", build_image(SYSTEM), []),
    ("the desktop app's shape: static VC runtime, Windows' UCRT", build_image(SYSTEM + UCRT), []),
    ("a plain cargo build", build_image(SYSTEM[:1] + ["VCRUNTIME140.dll"] + UCRT), ["VCRUNTIME140.dll"]),
    ("C++ in a dependency", build_image(["MSVCP140.dll", "KERNEL32.dll", "VCRUNTIME140_1.dll"]), ["MSVCP140.dll", "VCRUNTIME140_1.dll"]),
    ("lower case", build_image(["kernel32.dll", "vcruntime140.dll"]), ["vcruntime140.dll"]),
    ("a debug build", build_image(["KERNEL32.dll", "VCRUNTIME140D.dll", "ucrtbased.dll"]), ["VCRUNTIME140D.dll", "ucrtbased.dll"]),
    ("OpenMP and the concurrency runtime", build_image(["KERNEL32.dll", "VCOMP140.DLL", "concrt140.dll"]), ["VCOMP140.DLL", "concrt140.dll"]),
    ("an old runtime", build_image(["KERNEL32.dll", "MSVCR120.dll"]), ["MSVCR120.dll"]),
    ("a 32-bit program", build_image(["KERNEL32.dll", "VCRUNTIME140.dll"], bits=32), ["VCRUNTIME140.dll"]),
    ("a delay-loaded runtime", build_image(["KERNEL32.dll"], delayed=["MSVCP140.dll"]), ["MSVCP140.dll"]),
    ("a 32-bit delay-load, old form", build_image(["KERNEL32.dll"], delayed=["MSVCP140.dll"], bits=32, old_delay=True), ["MSVCP140.dll"]),
    ("delay-loaded system DLLs", build_image(["KERNEL32.dll"], delayed=["user32.dll", "shell32.dll"]), []),
    ("not a program", b"#!/bin/sh\necho hello\n" * 8, None),
    ("a truncated program", build_image(["KERNEL32.dll", "VCRUNTIME140.dll"])[:0x200], None),
    ("an import name outside the file", build_image(["KERNEL32.dll"])[:0x400] + bytes(0x10), None),
    ("an image that imports nothing", build_image([]), None),
]


def self_test() -> int:
    wrong: list[str] = []
    with tempfile.TemporaryDirectory() as tmp:
        for index, (name, image, planted) in enumerate(PLANTED):
            try:
                found: list[str] | None = forbidden(imports(image))
            except NotAProgram:
                found = None
            if found != planted:
                wrong.append(f"{name}: found {found if found is not None else 'a misreading'}, planted {planted if planted is not None else 'a misreading'}")
            path = os.path.join(tmp, f"{index}.exe")
            with open(path, "wb") as handle:
                handle.write(image)
            stdout, sys.stdout = sys.stdout, open(os.devnull, "w", encoding="utf-8")
            try:
                status = check([path])
            finally:
                sys.stdout.close()
                sys.stdout = stdout
            want = 2 if planted is None else 1 if planted else 0
            if status != want:
                wrong.append(f"{name}: the check exited {status}, not {want}")
        # Ordinary and delay-loaded imports are both listed, in table order.
        try:
            listed: object = imports(build_image(["KERNEL32.dll", "ws2_32.dll"], delayed=["user32.dll"]))
        except NotAProgram as error:
            listed = f"a misreading ({error})"
        if listed !=[("KERNEL32.dll", False), ("ws2_32.dll", False), ("user32.dll", True)]:
            wrong.append(f"the import list read as {listed}")

    if wrong:
        print("❌ self-test: a planted image was not judged as planted:")
        for line in wrong:
            print(f"     {line}")
        return 1
    faults = sum(1 for case in PLANTED if case[2] != [])
    print(f"✅ self-test: {len(PLANTED)} planted images, {faults} with a forbidden import or a misreading and each found, nothing else")
    return 0


def main() -> int:
    # Windows' Python writes a pipe in the ANSI code page, which has no ✅.
    sys.stdout.reconfigure(encoding="utf-8")
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    if not sys.argv[1:] or sys.argv[1].startswith("-"):
        print(__doc__.split("\n\n", 2)[1], file=sys.stderr)
        return 2
    return check(sys.argv[1:])


if __name__ == "__main__":
    sys.exit(main())
