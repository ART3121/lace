"""Sonda do Lace: o que o vvp precisa para carregar o cocotb deste Python.

Escreve uma linha de JSON no stdout. O Lace (crates/lace-core/src/cocotb.rs)
guarda o resultado em .lace/Temp/cocotb/probe.json e roda a sonda de novo
quando o Python do bundle muda. Funciona com o cocotb 1 (cocotb.config) e o
2 (cocotb_tools.config), que nomeiam a biblioteca e o ambiente de jeitos
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

libs = str(config.libs_dir)
vpi = str(config.lib_name_path("vpi", "icarus"))
if not os.path.isfile(vpi):
    found = [p for p in glob.glob(os.path.join(libs, "*cocotbvpi_icarus*")) if p.endswith(LIB_EXTS)]
    found.sort(key=lambda p: [p.endswith(ext) for ext in LIB_EXTS].index(True))
    if found:
        vpi = found[0]

print(
    json.dumps(
        {
            "version": cocotb.__version__,
            "modern": MODERN,
            "python": sys.executable,
            "libpython": find_libpython.find_libpython() or "",
            "entry_point": config.pygpi_entry_point() if MODERN else "",
            "vpi": vpi,
            "libs": libs,
            "sys_path": [
                p for p in sys.path if p and os.path.abspath(p) != HERE
            ],
        }
    )
)
