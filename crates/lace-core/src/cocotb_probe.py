"""Sonda do Lace: o que o simulador precisa para carregar o cocotb deste Python.

Escreve uma linha de JSON no stdout: a VPI do cocotb para o Icarus (o -m do
vvp), a do Verilator e o main.cpp que o modelo dele liga, a biblioteca do
Python e o resto do ambiente. O Lace (crates/lace-core/src/cocotb.rs) guarda
o resultado em .lace/Temp/cocotb/probe.json e roda a sonda de novo quando o
Python do bundle muda. Funciona com o cocotb 1 (cocotb.config) e o 2
(cocotb_tools.config), que nomeiam a biblioteca e o ambiente de jeitos
diferentes.
"""

import glob
import json
import os
import sys

import cocotb

try:
    import cocotb_tools.config as config  # cocotb 2

    MODERN = True
except ImportError:
    import cocotb.config as config  # cocotb 1

    MODERN = False

import find_libpython

HERE = os.path.dirname(os.path.abspath(__file__))

# As extensões de biblioteca, na ordem de preferência. O nome que o cocotb dá
# (lib_name_path) pode não existir: no egg do OSS CAD Suite, a VPI do Icarus
# é .vpl, e cada biblioteca tem ao lado um .py de carga do setuptools.
LIB_EXTS = (".vpl", ".so", ".dylib", ".dll", ".vpi")

# A VPI do Verilator entra na ligação do modelo: compartilhada no Linux e no
# macOS, estática no Windows (o cocotb do MSYS2 só traz o .a).
VERILATOR_LIB_EXTS = (".so", ".dylib", ".a", ".dll", ".vpl")

libs = str(config.libs_dir)


def library(simulator, extensions):
    """A VPI do cocotb para o simulador, ou "" se ele não a traz."""
    named = str(config.lib_name_path("vpi", simulator))
    if os.path.isfile(named):
        return named
    found = [
        p
        for p in glob.glob(os.path.join(libs, "*cocotbvpi_" + simulator + "*"))
        if p.endswith(extensions)
    ]
    found.sort(key=lambda p: [p.endswith(ext) for ext in extensions].index(True))
    return found[0] if found else ""


vpi = library("icarus", LIB_EXTS)
verilator_vpi = library("verilator", VERILATOR_LIB_EXTS)
verilator_main = os.path.join(str(config.share_dir), "lib", "verilator", "verilator.cpp")
if not os.path.isfile(verilator_main):
    verilator_main = ""

print(
    json.dumps(
        {
            "version": cocotb.__version__,
            "modern": MODERN,
            "python": sys.executable,
            "libpython": find_libpython.find_libpython() or "",
            "entry_point": config.pygpi_entry_point() if MODERN else "",
            "vpi": vpi,
            "verilator_vpi": verilator_vpi,
            "verilator_main": verilator_main,
            "libs": libs,
            "sys_path": [
                p for p in sys.path if p and os.path.abspath(p) != HERE
            ],
        }
    )
)
