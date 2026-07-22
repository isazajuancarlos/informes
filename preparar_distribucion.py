"""Prepara `vendor/` con la copia de Quipu que se entrega al cliente.

# Por qué existe

El despliegue del cliente **no debe instalar `quipu-crypto` desde PyPI**.

No es una cuestión de metadatos ni de estética: si el cliente obtiene la copia de
un canal público, la obtiene **bajo AGPL-3.0**, y su uso en red del conjunto
activaría el artículo 13 — quedaría obligado a ofrecer el código fuente del
sistema completo a los usuarios que interactúen con él por red. Un cliente
corporativo con asesoría competente detecta eso en la debida diligencia.

La Cláusula 9.2 del EULA entrega esa copia bajo **licencia comercial**, que es
para lo que existe la doble licencia. Este script es la mitad técnica de esa
cláusula: pone la rueda dentro de la distribución para que `pip` no tenga que ir
a buscarla fuera. La 9.3 le prohíbe al cliente sustituirla por la pública.

# Qué NO hace, y es deliberado

No reconstruye la rueda con otros metadatos de licencia. Sería peor: dos
artefactos llamados `quipu-crypto X.Y.Z` con contenido distinto, y quien compare
hashes en una auditoría encontraría una discrepancia que huele mucho peor que un
clasificador. La licencia bajo la que se recibe una copia la fija el **contrato
de entrega**, no una cadena en `METADATA`. Y la rueda ya entrega los dos textos:
`LICENSE` (AGPL) y `LICENSE-COMMERCIAL`.

# Uso

    python preparar_distribucion.py                 # plataforma actual
    python preparar_distribucion.py --version 0.9.1
    python preparar_distribucion.py --plataforma manylinux_2_34_x86_64

`vendor/` NO se versiona: se genera al empaquetar. Ver .gitignore.
"""
from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parent
VENDOR = RAIZ / "vendor"
PAQUETE = "quipu-crypto"


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--version", default="", help="Versión exacta (por defecto, la última)")
    p.add_argument("--plataforma", default="",
                   help="Etiqueta de plataforma para descarga cruzada, "
                        "p. ej. manylinux_2_34_x86_64")
    p.add_argument("--python", default="", help="Versión de Python destino, p. ej. 3.11")
    args = p.parse_args()

    VENDOR.mkdir(exist_ok=True)

    especificador = f"{PAQUETE}=={args.version}" if args.version else PAQUETE
    cmd = [sys.executable, "-m", "pip", "download", especificador,
           "--dest", str(VENDOR), "--no-deps"]
    if args.plataforma:
        # Con `--platform` pip exige `--only-binary`: no puede compilar para
        # otra plataforma, solo bajar una rueda ya construida.
        cmd += ["--platform", args.plataforma, "--only-binary", ":all:"]
    if args.python:
        cmd += ["--python-version", args.python, "--only-binary", ":all:"]

    print("Descargando la rueda a vendor/ …")
    r = subprocess.run(cmd)
    if r.returncode != 0:
        print("ERROR: no se pudo descargar la rueda.", file=sys.stderr)
        return 1

    ruedas = sorted(VENDOR.glob("quipu_crypto-*.whl"))
    if not ruedas:
        print("ERROR: no quedó ninguna rueda en vendor/.", file=sys.stderr)
        return 1

    print("\nEn vendor/:")
    for w in ruedas:
        print(f"  {w.name}  ({w.stat().st_size // 1024} KiB)")

    if len(ruedas) > 1:
        # Dos ruedas del mismo paquete es una trampa: `--find-links` elegiría
        # una y nadie sabría cuál se entregó.
        print("\nAVISO: hay más de una rueda de quipu-crypto en vendor/. "
              "Deja solo la que se entrega, o pip elegirá por su cuenta.",
              file=sys.stderr)
        return 1

    # Comprobar que lo descargado SIRVE, no solo que existe: una rueda de otra
    # plataforma se descarga igual de bien y falla al instalar en el cliente.
    print("\nComprobando que la rueda instala en este intérprete…")
    prueba = subprocess.run(
        [sys.executable, "-m", "pip", "install", "--dry-run", "--no-index",
         "--find-links", str(VENDOR), PAQUETE],
        capture_output=True, text=True,
    )
    if prueba.returncode == 0:
        print("  OK: instalable sin tocar PyPI.")
    else:
        print("  AVISO: no instala en ESTE intérprete. Es esperable si la "
              "descargaste para otra plataforma con --plataforma; compruébalo "
              "en la máquina destino antes de entregar.")

    print("\nListo. `requirements-firma.txt` instalará desde vendor/ y NO desde PyPI.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
