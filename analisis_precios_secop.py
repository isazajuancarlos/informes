"""Ancla los precios de INFORMES contra contratación pública real (SECOP II).

Reproduce el análisis completo desde la fuente. No hay números pegados a mano en
ninguna parte: si SECOP cambia, se vuelve a correr y los números cambian con él.

    python analisis_precios_secop.py            # descarga y analiza
    python analisis_precios_secop.py --cache    # reutiliza lo ya descargado

# LA CORRECCIÓN METODOLÓGICA QUE MOTIVA ESTE SCRIPT

El primer análisis (2026-07-18) pidió a la API **los 200 contratos de mayor
valor** y calculó la mediana sobre ellos. Esa no es la mediana del mercado: es
la del tramo alto, y salía inflada en un orden de magnitud. De ahí se concluyó
que los precios propuestos estaban «bajos por un orden de magnitud».

**Esa conclusión era falsa, y de las caras**: subir precios sobre una medición
sesgada habría dejado el producto fuera de mercado sin que nadie supiera por qué.

Aquí se descarga el UNIVERSO completo que cumple el filtro y se calculan los
percentiles sobre él. Cuando una muestra se ordena por la variable que se va a
medir, deja de ser una muestra.

# LOS DOS MERCADOS QUE HAY QUE MIRAR

1. **Software de seguimiento/monitoreo** — el comparable directo.
2. **Contratar una PERSONA** para hacer ese seguimiento a mano — el competidor
   REAL. Las entidades no comparan este producto contra otro software: lo
   comparan contra el analista que ya tienen. Por eso se anualiza: los contratos
   de persona duran una mediana de 180 días, y compararlos sin normalizar
   subestimaría el coste anual a la mitad.
"""
from __future__ import annotations

import argparse
import json
import pickle
import re
import statistics
import sys
import time
import urllib.parse
import urllib.request
from pathlib import Path

BASE = "https://www.datos.gov.co/resource/jbjy-vk9h.json"  # SECOP II, contratos
CACHE = Path(__file__).resolve().parent / ".cache_secop"

#: Software de seguimiento, excluyendo la contratación de personas.
FILTRO_SOFTWARE = (
    "((upper(descripcion_del_proceso) like '%SOFTWARE%' "
    "  or upper(descripcion_del_proceso) like '%SISTEMA DE INFORMACI%') "
    "and (upper(descripcion_del_proceso) like '%SEGUIMIENTO%' "
    "  or upper(descripcion_del_proceso) like '%MONITOREO%') "
    "and not upper(descripcion_del_proceso) like '%PRESTACI_N DE SERVICIOS PROFESIONALES%')"
)

#: Personas contratadas para el seguimiento de informes: el competidor real.
FILTRO_PERSONAS = (
    "(upper(descripcion_del_proceso) like '%PRESTACI_N DE SERVICIOS PROFESIONALES%' "
    "and (upper(descripcion_del_proceso) like '%SEGUIMIENTO%' "
    "  or upper(descripcion_del_proceso) like '%MONITOREO%') "
    "and (upper(descripcion_del_proceso) like '%INFORME%' "
    "  or upper(descripcion_del_proceso) like '%REPORTE%'))"
)

CAMPOS = ("valor_del_contrato,duraci_n_del_contrato,nombre_entidad,"
          "modalidad_de_contratacion,descripcion_del_proceso")


def descargar(where: str, etiqueta: str, usar_cache: bool) -> list[dict]:
    CACHE.mkdir(exist_ok=True)
    archivo = CACHE / f"{etiqueta}.pkl"
    if usar_cache and archivo.exists():
        return pickle.load(archivo.open("rb"))

    filas, offset = [], 0
    while True:
        # `$order` NO es cosmético: sin un orden explícito, la paginación de
        # Socrata no garantiza estabilidad entre lotes y se repiten o pierden
        # filas. Medido el 2026-07-21: dos corridas seguidas daban medianas de
        # $37,1M y $35,6M sobre el MISMO filtro. Un 4% de deriva es intolerable
        # en un documento de precios, y lo peor es que no se nota.
        q = urllib.parse.urlencode(
            {"$select": CAMPOS, "$where": where, "$order": ":id",
             "$limit": 500, "$offset": offset})
        lote = None
        for intento in range(3):
            try:
                with urllib.request.urlopen(f"{BASE}?{q}", timeout=180) as r:
                    lote = json.load(r)
                break
            except Exception as e:  # noqa: BLE001
                print(f"  reintento {intento+1} (offset {offset}): {e}", file=sys.stderr)
                time.sleep(5)
        if lote is None:
            raise SystemExit(f"No se pudo descargar {etiqueta}: se aborta en vez de "
                             "analizar una muestra incompleta sin decirlo.")
        filas.extend(lote)
        if len(lote) < 500:
            break
        offset += 500
    pickle.dump(filas, archivo.open("wb"))
    return filas


def dias(txt) -> int | None:
    """'54 Dia(s)' o '6 Mes(es)' -> días. Devuelve None si no se reconoce.

    NO se inventa una duración por defecto: un contrato cuya duración no se
    entiende se descarta y se informa cuántos fueron. Rellenar con una media
    metería ruido invisible en la anualización (directiva 20).
    """
    if not txt:
        return None
    m = re.match(r"\s*(\d+)\s*(\w+)", str(txt))
    if not m:
        return None
    n, unidad = int(m.group(1)), m.group(2).lower()
    if unidad.startswith("dia"):
        return n
    if unidad.startswith("mes"):
        return n * 30
    if unidad.startswith("a"):
        return n * 365
    return None


def valores(filas) -> list[float]:
    out = []
    for f in filas:
        try:
            v = float(f.get("valor_del_contrato") or 0)
        except (TypeError, ValueError):
            continue
        if v > 0:
            out.append(v)
    return sorted(out)


def M(x: float) -> str:
    return f"${x/1e6:,.1f}M"


def percentiles(vals: list[float]) -> dict[str, float]:
    return {n: vals[min(int(len(vals) * p), len(vals) - 1)]
            for n, p in (("p25", .25), ("mediana", .50), ("p75", .75), ("p90", .90))}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--cache", action="store_true", help="reutiliza lo descargado")
    args = ap.parse_args()

    print("Descargando SECOP II…", file=sys.stderr)
    soft = descargar(FILTRO_SOFTWARE, "software", args.cache)
    pers = descargar(FILTRO_PERSONAS, "personas", args.cache)

    # --- 1. Software de seguimiento -------------------------------------
    vs = valores(soft)
    print(f"\n1. SOFTWARE DE SEGUIMIENTO — universo completo, {len(vs):,} contratos")
    print(f"   (de {len(soft):,} descargados; {len(soft)-len(vs):,} sin valor legible)")
    for n, v in percentiles(vs).items():
        print(f"     {n:>8}  {M(v)}")

    # --- 2. El competidor real: una persona ------------------------------
    pares = []
    sin_duracion = 0
    for f in pers:
        try:
            v = float(f.get("valor_del_contrato") or 0)
        except (TypeError, ValueError):
            continue
        if v <= 0:
            continue
        d = dias(f.get("duraci_n_del_contrato"))
        if d and 30 <= d <= 1825:
            pares.append(v * 365 / d)
        else:
            sin_duracion += 1
    pares.sort()
    print(f"\n2. UNA PERSONA para ese seguimiento — {len(pares):,} contratos anualizados")
    print(f"   ({sin_duracion:,} descartados por duración ilegible o fuera de rango)")
    for n, v in percentiles(pares).items():
        print(f"     {n:>8}  {M(v)} / año")

    # --- 3. Por dónde se contrata ----------------------------------------
    from collections import defaultdict
    por_mod = defaultdict(int)
    for f in soft:
        por_mod[f.get("modalidad_de_contratacion", "?")] += 1
    total = sum(por_mod.values())
    print(f"\n3. POR DÓNDE ENTRA — modalidad de los {total:,} contratos de software")
    for mod, n in sorted(por_mod.items(), key=lambda kv: -kv[1])[:5]:
        print(f"     {100*n/total:>5.1f}%  {mod}")

    # --- 4. El anclaje ----------------------------------------------------
    mediana_persona = percentiles(pares)["mediana"]
    print("\n4. ANCLAJE DE LOS PRECIOS PROPUESTOS")
    for nombre, precio in (("Esencial", 12e6), ("Profesional", 28e6), ("Corporativa", 55e6)):
        pct_soft = 100 * sum(1 for v in vs if v < precio) / len(vs)
        meses = precio / (mediana_persona / 12)
        print(f"     {nombre:<12} {M(precio):>9}  = percentil {pct_soft:4.0f} del software"
              f"  ·  {meses:4.1f} meses de una persona")
    print(f"\n     Mediana anual de una persona: {M(mediana_persona)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
