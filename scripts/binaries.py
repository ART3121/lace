"""Dependências de bibliotecas dinâmicas de binários ELF, Mach-O e PE.

Usado pelo scripts/bundle.py para dividir o OSS CAD Suite por ferramenta: a
parte de cada ferramenta é o que ela executa, mais o fecho das bibliotecas
que esses binários carregam. Só a biblioteca padrão do Python, para rodar
igual nas três plataformas (e analisar o pacote de uma plataforma em outra).

    kind(path)          -> "elf" | "macho" | "pe" | None
    dependencies(path)  -> Deps(needed=[...], rpaths=[...])
"""

import struct
from dataclasses import dataclass, field


@dataclass
class Deps:
    """O que um binário pede ao carregador."""

    needed: list = field(default_factory=list)
    rpaths: list = field(default_factory=list)


def kind(path):
    """O formato do binário, pelo número mágico; None se não for binário."""
    try:
        with open(path, "rb") as f:
            head = f.read(8)
    except OSError:
        return None
    if head[:4] == b"\x7fELF":
        return "elf"
    if head[:4] in (b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe"):
        return "macho"
    if head[:4] in (b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf"):
        # O mesmo número mágico das classes Java: um binário universal tem
        # poucas arquiteturas.
        (count,) = struct.unpack(">I", head[4:8])
        return "macho" if 0 < count < 32 else None
    if head[:2] == b"MZ":
        return "pe"
    return None


def dependencies(path):
    """As bibliotecas que o binário pede, e os rpaths dele."""
    with open(path, "rb") as f:
        data = f.read()
    k = kind(path)
    if k == "elf":
        return _elf(data)
    if k == "macho":
        return _macho(data)
    if k == "pe":
        return _pe(data)
    return Deps()


def _cstr(data, offset):
    end = data.find(b"\0", offset)
    return data[offset : end if end >= 0 else len(data)].decode("utf-8", "replace")


# ------------------------------------------------------------------ ELF


def _elf(data):
    is64 = data[4] == 2
    end = "<" if data[5] == 1 else ">"
    if is64:
        phoff, = struct.unpack_from(end + "Q", data, 0x20)
        phentsize, phnum = struct.unpack_from(end + "HH", data, 0x36)
    else:
        phoff, = struct.unpack_from(end + "I", data, 0x1C)
        phentsize, phnum = struct.unpack_from(end + "HH", data, 0x2A)

    loads, dynamic = [], None
    for i in range(phnum):
        off = phoff + i * phentsize
        if is64:
            p_type, _flags, p_offset, p_vaddr, _paddr, p_filesz = struct.unpack_from(end + "IIQQQQ", data, off)
        else:
            p_type, p_offset, p_vaddr, _paddr, p_filesz = struct.unpack_from(end + "IIIII", data, off)
        if p_type == 1:  # PT_LOAD
            loads.append((p_vaddr, p_offset, p_filesz))
        elif p_type == 2:  # PT_DYNAMIC
            dynamic = (p_offset, p_filesz)
    if dynamic is None:
        return Deps()  # estático

    def to_offset(vaddr):
        for v, o, size in loads:
            if v <= vaddr < v + size:
                return vaddr - v + o
        return vaddr

    entry = struct.Struct(end + ("qQ" if is64 else "iI"))
    needed_offsets, rpath_offsets, strtab = [], [], None
    off, stop = dynamic[0], dynamic[0] + dynamic[1]
    while off + entry.size <= stop:
        tag, val = entry.unpack_from(data, off)
        off += entry.size
        if tag == 0:
            break
        if tag == 1:  # DT_NEEDED
            needed_offsets.append(val)
        elif tag == 5:  # DT_STRTAB
            strtab = to_offset(val)
        elif tag in (15, 29):  # DT_RPATH, DT_RUNPATH
            rpath_offsets.append(val)
    if strtab is None:
        return Deps()
    rpaths = []
    for o in rpath_offsets:
        rpaths += [p for p in _cstr(data, strtab + o).split(":") if p]
    return Deps([_cstr(data, strtab + o) for o in needed_offsets], rpaths)


# --------------------------------------------------------------- Mach-O

_LC_DYLIBS = {0xC, 0x80000018, 0x8000001F, 0x20, 0x80000023}
_LC_RPATH = 0x8000001C
_CPU_ARM64 = 0x0100000C


def _macho(data):
    magic = data[:4]
    if magic in (b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf"):
        wide = magic == b"\xca\xfe\xba\xbf"
        (count,) = struct.unpack_from(">I", data, 4)
        arch = struct.Struct(">iiQQI" if wide else ">iiIII")
        slices = []
        for i in range(count):
            cputype, _sub, offset, size, _align = arch.unpack_from(data, 8 + i * arch.size)
            slices.append((cputype, data[offset : offset + size]))
        # O Solar só roda em arm64; sem essa fatia, a união de todas.
        chosen = [s for c, s in slices if c == _CPU_ARM64] or [s for _, s in slices]
        deps = Deps()
        for s in chosen:
            d = _macho(s)
            deps.needed += [n for n in d.needed if n not in deps.needed]
            deps.rpaths += [r for r in d.rpaths if r not in deps.rpaths]
        return deps

    is64 = magic == b"\xcf\xfa\xed\xfe"
    ncmds, _sizeofcmds = struct.unpack_from("<II", data, 16)
    off = 32 if is64 else 28
    deps = Deps()
    for _ in range(ncmds):
        cmd, cmdsize = struct.unpack_from("<II", data, off)
        if cmd in _LC_DYLIBS or cmd == _LC_RPATH:
            (name_off,) = struct.unpack_from("<I", data, off + 8)
            text = _cstr(data[: off + cmdsize], off + name_off)
            (deps.rpaths if cmd == _LC_RPATH else deps.needed).append(text)
        off += cmdsize
    return deps


# ------------------------------------------------------------------- PE


def _pe(data):
    (lfanew,) = struct.unpack_from("<I", data, 0x3C)
    if data[lfanew : lfanew + 4] != b"PE\0\0":
        return Deps()
    coff = lfanew + 4
    _machine, nsections = struct.unpack_from("<HH", data, coff)
    (opt_size,) = struct.unpack_from("<H", data, coff + 16)
    opt = coff + 20
    (opt_magic,) = struct.unpack_from("<H", data, opt)
    dirs = opt + (112 if opt_magic == 0x20B else 96)
    (ndirs,) = struct.unpack_from("<I", data, dirs - 4)

    def directory(index):
        if index >= ndirs:
            return 0, 0
        return struct.unpack_from("<II", data, dirs + 8 * index)

    sections = []
    table = opt + opt_size
    for i in range(nsections):
        vsize, vaddr, rawsize, rawptr = struct.unpack_from("<IIII", data, table + 40 * i + 8)
        sections.append((vaddr, max(vsize, rawsize), rawptr))

    def to_offset(rva):
        for vaddr, size, rawptr in sections:
            if vaddr <= rva < vaddr + size:
                return rva - vaddr + rawptr
        return None

    names = []
    # Tabela de importação (diretório 1): descritores de 20 bytes.
    rva, _size = directory(1)
    off = to_offset(rva) if rva else None
    while off is not None and off + 20 <= len(data):
        fields = struct.unpack_from("<IIIII", data, off)
        if not any(fields):
            break
        name = to_offset(fields[3])
        if name is not None:
            names.append(_cstr(data, name))
        off += 20
    # Importação atrasada (diretório 13): descritores de 32 bytes.
    rva, _size = directory(13)
    off = to_offset(rva) if rva else None
    while off is not None and off + 32 <= len(data):
        fields = struct.unpack_from("<IIIIIIII", data, off)
        if not any(fields):
            break
        name = to_offset(fields[1])
        if name is not None:
            names.append(_cstr(data, name))
        off += 32
    return Deps(names)
