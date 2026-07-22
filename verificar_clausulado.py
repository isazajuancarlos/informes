"""Comprueba que el modelo que se entrega al cliente no se quede atrás.

`EULA_BORRADOR.md` es el documento de trabajo: lleva las notas ⚑ y la sección
«Notas al redactor», que son de uso interno. `MODELO_LICENCIA_INFORMES.md` es el
MISMO clausulado sin esas notas, que es lo que se entrega a un tercero.

Dos copias del mismo texto que hay que mantener sincronizadas a mano es una
divergencia esperando a ocurrir — y ya ocurrió: el 2026-07-21 el modelo para
cliente no tenía la Cláusula 3.5 que sí estaba en el de trabajo. En un contrato
eso no es un detalle de formato: significa entregar un clausulado distinto del
que uno cree estar entregando.

Este script no impide la divergencia; la vuelve **visible**. Es lo mínimo
mientras los dos archivos existan por separado.

    python verificar_clausulado.py

Salida: 0 si concuerdan · 1 si el modelo se quedó atrás.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parent
TRABAJO = RAIZ / "EULA_BORRADOR.md"
CLIENTE = RAIZ / "MODELO_LICENCIA_INFORMES.md"

#: Encabezado a partir del cual empieza el material de uso interno del documento
#: de trabajo. Todo lo que venga después NO debe estar en la versión del cliente.
CORTE_INTERNO = "NOTAS AL REDACTOR"

#: `**3.5. Ampliación del alcance.**` -> ("3.5", "Ampliación del alcance")
NUMERADA = re.compile(r"^\*\*(\d+(?:\.\d+)*)\.\s+([^.*]+)\.?\*\*", re.MULTILINE)


def clausulas(ruta: Path, cortar_internas: bool) -> dict[str, str]:
    texto = ruta.read_text(encoding="utf-8")
    if cortar_internas:
        corte = texto.find(CORTE_INTERNO)
        if corte != -1:
            texto = texto[:corte]
    return {n: t.strip() for n, t in NUMERADA.findall(texto)}


def main() -> int:
    for f in (TRABAJO, CLIENTE):
        if not f.exists():
            print(f"ERROR: no existe {f.name}", file=sys.stderr)
            return 1

    trabajo = clausulas(TRABAJO, cortar_internas=True)
    cliente = clausulas(CLIENTE, cortar_internas=True)

    faltan = sorted(set(trabajo) - set(cliente), key=_orden)
    sobran = sorted(set(cliente) - set(trabajo), key=_orden)
    distintas = sorted(
        (n for n in set(trabajo) & set(cliente) if trabajo[n] != cliente[n]),
        key=_orden,
    )

    print(f"{TRABAJO.name}: {len(trabajo)} cláusulas numeradas")
    print(f"{CLIENTE.name}: {len(cliente)} cláusulas numeradas\n")

    if faltan:
        print("FALTAN en el modelo que se entrega al cliente:")
        for n in faltan:
            print(f"  {n}  {trabajo[n]}")
    if sobran:
        print("\nSOBRAN en el modelo del cliente (no están en el de trabajo):")
        for n in sobran:
            print(f"  {n}  {cliente[n]}")
    if distintas:
        print("\nMISMO NÚMERO, DISTINTO TÍTULO:")
        for n in distintas:
            print(f"  {n}\n     trabajo: {trabajo[n]}\n     cliente: {cliente[n]}")

    if faltan or sobran or distintas:
        print(f"\nDIVERGEN. Sincronice antes de entregar el clausulado.")
        return 1

    print("Los dos documentos tienen el mismo clausulado numerado.")
    return 0


def _orden(numero: str) -> tuple:
    return tuple(int(p) for p in numero.split("."))


if __name__ == "__main__":
    sys.exit(main())
