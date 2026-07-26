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

Se comparan tres cosas, y la tercera es la que importa: qué cláusulas hay, cómo
se titulan y **qué dicen**. Durante un tiempo solo se compararon las dos
primeras, y eso dejaba pasar justo la divergencia peligrosa: dos cláusulas con
el mismo número y el mismo título que dicen cosas distintas. En un contrato,
«3.5 Ampliación del alcance» presente en ambos documentos no significa nada si
el plazo o el procedimiento cambian de uno a otro — y era invisible.

Del documento de trabajo se ignoran las notas ⚑ (van en blockquote) porque son
internas por diseño, y los saltos de línea, que son formato.

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


def _sin_material_interno(ruta: Path, cortar_internas: bool) -> str:
    texto = ruta.read_text(encoding="utf-8")
    if cortar_internas:
        corte = texto.find(CORTE_INTERNO)
        if corte != -1:
            texto = texto[:corte]
    return texto


def clausulas(ruta: Path, cortar_internas: bool) -> dict[str, str]:
    """Número -> título."""
    texto = _sin_material_interno(ruta, cortar_internas)
    return {n: t.strip() for n, t in NUMERADA.findall(texto)}


def cuerpos(ruta: Path, cortar_internas: bool) -> dict[str, str]:
    """Número -> texto normalizado de la cláusula.

    Se descarta lo que es formato o material interno, para que la comparación
    señale diferencias de CONTENIDO y no ruido tipográfico:

      - las líneas en blockquote (`>`), que son las notas ⚑ del documento de
        trabajo y no deben viajar al cliente;
      - los separadores `---` y los encabezados `#`, que pertenecen a la
        maquetación y aparecen al final de una cláusula solo según dónde caiga
        la siguiente sección;
      - los saltos de línea y los espacios repetidos.
    """
    texto = _sin_material_interno(ruta, cortar_internas)
    marcas = list(NUMERADA.finditer(texto))
    fuera: dict[str, str] = {}
    for i, m in enumerate(marcas):
        fin = marcas[i + 1].start() if i + 1 < len(marcas) else len(texto)
        lineas = []
        for linea in texto[m.start():fin].splitlines():
            limpia = linea.strip()
            if limpia.startswith(">") or limpia.startswith("#") or set(limpia) == {"-"}:
                continue
            lineas.append(limpia)
        fuera[m.group(1)] = " ".join(" ".join(lineas).split())
    return fuera


def _primera_diferencia(a: str, b: str) -> str:
    """Dónde se separan los dos textos, con contexto suficiente para leerlo."""
    i = next((k for k in range(min(len(a), len(b))) if a[k] != b[k]), min(len(a), len(b)))
    ini = max(0, i - 60)
    return (f"     trabajo: …{a[ini:i + 80]}…\n"
            f"     cliente: …{b[ini:i + 80]}…")


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

    # El contenido, que es lo que el cliente firma. Una cláusula puede coincidir
    # en número y título y decir otra cosa: eso es lo que hay que cazar.
    cuerpo_trabajo = cuerpos(TRABAJO, cortar_internas=True)
    cuerpo_cliente = cuerpos(CLIENTE, cortar_internas=True)
    redactadas_distinto = sorted(
        (n for n in set(cuerpo_trabajo) & set(cuerpo_cliente)
         if cuerpo_trabajo[n] != cuerpo_cliente[n]),
        key=_orden,
    )
    if redactadas_distinto:
        print("\nMISMO TÍTULO, TEXTO DISTINTO — es la divergencia grave:")
        for n in redactadas_distinto:
            print(f"  {n}  {trabajo.get(n, '')}")
            print(_primera_diferencia(cuerpo_trabajo[n], cuerpo_cliente[n]))

    if faltan or sobran or distintas or redactadas_distinto:
        print("\nDIVERGEN. Sincronice antes de entregar el clausulado.")
        return 1

    print(f"Los dos documentos concuerdan: {len(trabajo)} cláusulas, "
          "mismo número, mismo título y mismo texto.")
    return 0


def _orden(numero: str) -> tuple:
    return tuple(int(p) for p in numero.split("."))


if __name__ == "__main__":
    sys.exit(main())
