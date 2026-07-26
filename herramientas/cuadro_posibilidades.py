"""Cuadro de todo lo que el sistema puede expresar. EJECUTADO, no escrito.

    python herramientas/cuadro_posibilidades.py

Cada fila se calcula con el motor de producción y trae el número de
vencimientos que genera de verdad en un año. Un cuadro escrito a mano envejece
y acaba prometiendo lo que el sistema ya no hace; este se cae si algo deja de
funcionar.
"""
from __future__ import annotations

import os
import sys
from datetime import date, timedelta

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.environ.setdefault("DATABASE_URL", "sqlite:///:memory:")

from app.services.scheduler import GeneradorTareas as G  # noqa: E402
from app.services.validar_regla import validar  # noqa: E402

ANIO = 2026
I = {"fecha_inicio": f"{ANIO}-01-01"}
TODOS = list(range(1, 13))

#: (dimensión, qué expresa, regla, cómo se lee)
FILAS = [
    # ---- AÑO ----
    ("AÑO", "cada año",
     {"modo": "ESPECIFICO", "meses": [3], "dia_corte": 15, **I},
     "15 de marzo, todos los años"),
    ("AÑO", "cada N años",
     {"modo": "ESPECIFICO", "meses": [3], "dia_corte": 15, "cada_n_anios": 2, **I},
     "15 de marzo, un año sí y otro no"),

    # ---- MES ----
    ("MES", "todos los meses",
     {"modo": "ESPECIFICO", "meses": TODOS, "dia_corte": 15, **I}, "el 15 de cada mes"),
    ("MES", "algunos meses",
     {"modo": "ESPECIFICO", "meses": [3, 6, 9, 12], "dia_corte": 15, **I},
     "trimestral, el 15"),
    ("MES", "un solo mes",
     {"modo": "ESPECIFICO", "meses": [11], "dia_corte": 30, **I},
     "anual, el 30 de noviembre"),

    # ---- DÍA DEL MES ----
    ("DÍA", "un número fijo",
     {"modo": "ESPECIFICO", "meses": TODOS, "dia_corte": 5, **I}, "el día 5"),
    ("DÍA", "varios números fijos",
     {"modo": "ESPECIFICO", "meses": TODOS, "dias_corte": [7, 20], **I},
     "los días 7 y 20 — quincenal"),
    ("DÍA", "el último del mes",
     {"modo": "ESPECIFICO", "meses": TODOS, "es_ultimo_dia": True, **I},
     "28, 30 o 31 según el mes"),
    ("DÍA", "un número que algún mes no tiene",
     {"modo": "ESPECIFICO", "meses": TODOS, "dia_corte": 30,
      "usar_ultimo_dia_en_meses_cortos": True, **I},
     "el 30, y el 28 en febrero — hay que declararlo"),
    ("DÍA", "el n-ésimo día HÁBIL del mes",
     {"modo": "ESPECIFICO", "meses": TODOS, "dia_habil": 5, **I},
     "«los primeros 5 días hábiles»"),
    ("DÍA", "un ordinal de día de semana",
     {"modo": "ESPECIFICO", "meses": TODOS, "dia_semana_mes": 1,
      "semana_del_mes": -1, **I}, "el último martes de cada mes"),

    # ---- DÍA DE LA SEMANA ----
    ("SEMANA", "un día de la semana",
     {"modo": "SEMANAL", "dias_semana": [0], **I}, "todos los lunes"),
    ("SEMANA", "varios días de la semana",
     {"modo": "SEMANAL", "dias_semana": [2, 4], **I}, "miércoles y viernes"),
    ("SEMANA", "el n-ésimo día HÁBIL de la semana",
     {"modo": "SEMANAL", "dia_habil_semana": 4, **I},
     "el 4º hábil: jueves, o viernes si hubo festivo"),

    ("SEMANA", "cada N semanas, en el día que se elija",
     {"modo": "SEMANAL", "dias_semana": [1], "cada_n_semanas": 2, **I},
     "cada dos martes — «bisemanal» en su sentido de cada dos semanas"),

    # ---- JORNADA ----
    ("JORNADA", "de domingo a domingo",
     {"modo": "CICLICO", "intervalo": 1, **I}, "diario, sin excepciones"),
    ("JORNADA", "de lunes a viernes",
     {"modo": "SEMANAL", "dias_semana": [0, 1, 2, 3, 4], **I},
     "jornada de entidad pública"),
    ("JORNADA", "de lunes a sábado",
     {"modo": "SEMANAL", "dias_semana": [0, 1, 2, 3, 4, 5], **I},
     "jornada de empresa privada"),
    ("JORNADA", "omitiendo los festivos",
     {"modo": "SEMANAL", "dias_semana": [0, 1, 2, 3, 4], "omitir_festivos": True, **I},
     "el festivo no genera informe"),

    # ---- CICLO ----
    ("CICLO", "cada N días",
     {"modo": "CICLICO", "intervalo": 45, **I}, "cada 45 días desde la vigencia"),
    ("CICLO", "una sola vez",
     {"modo": "CICLICO", "intervalo": 0, **I}, "obligación no recurrente"),

    # ---- HORA ----
    ("HORA", "varias veces al día",
     {"modo": "HOURLY", "hora_inicio": 8, "hora_fin": 18, "intervalo_horas": 2, **I},
     "de 8 a 18, cada 2 horas"),
    ("HORA", "turno que cruza la medianoche",
     {"modo": "HOURLY", "hora_inicio": 18, "hora_fin": 8, "intervalo_horas": 2, **I},
     "de 18 a 8 del día siguiente"),

    # ---- SIN PATRÓN ----
    ("LIBRE", "fechas sueltas",
     {"modo": "FECHAS", "fechas": [f"{ANIO}-03-15", f"{ANIO}-07-04"], **I},
     "cuando ningún patrón encaja"),

    # ---- SI CAE EN DÍA NO HÁBIL ----
    ("AJUSTE", "se entrega ese mismo día",
     {"modo": "ESPECIFICO", "meses": [1], "dia_corte": 1, **I},
     "el vencimiento no se toca"),
    ("AJUSTE", "entrega anticipada",
     {"modo": "ESPECIFICO", "meses": [1], "dia_corte": 1,
      "ajuste_habil_anterior": True, **I},
     "se entrega antes; NO concede plazo"),
    ("AJUSTE", "plazo de gracia",
     {"modo": "ESPECIFICO", "meses": [1], "dia_corte": 1, "ajuste_habil": True, **I},
     "se admite entregar después"),
]


def al_anio(regla):
    total, dia = 0, date(ANIO, 1, 1)
    while dia <= date(ANIO, 12, 31):
        total += len(G.obtener_vencimientos_para_fecha(dia, regla))
        dia += timedelta(days=1)
    return total


def a_excel(destino="cuadro_posibilidades.xlsx") -> int:
    """El mismo cuadro, en una hoja. Las cifras se calculan al generarlo.

    No es un documento que alguien mantenga a mano: se regenera, y si algo dejó
    de funcionar la fila sale marcada. Un cuadro escrito envejece y acaba
    prometiendo lo que el sistema ya no hace.
    """
    from openpyxl import Workbook
    from openpyxl.styles import Alignment, Font, PatternFill

    wb = Workbook()
    ws = wb.active
    ws.title = "Posibilidades"

    ws["A1"] = "Periodicidades que el sistema puede manejar"
    ws["A1"].font = Font(size=14, bold=True)
    ws["A2"] = (f"Generado ejecutando el motor sobre el año {ANIO}. "
                f"Cada cifra es el número de vencimientos que la regla produce "
                f"de verdad, no una estimación.")
    ws["A2"].font = Font(italic=True, size=9)
    ws.merge_cells("A2:E2")

    cab = ["Dimensión", "Posibilidad", "Vencimientos al año", "Cómo se lee",
           "Se puede crear"]
    for col, texto in enumerate(cab, start=1):
        c = ws.cell(row=4, column=col, value=texto)
        c.font = Font(bold=True, color="FFFFFF")
        c.fill = PatternFill("solid", start_color="374151")
        c.alignment = Alignment(horizontal="center", vertical="center")

    fila = 5
    malas = 0
    for dimension, posibilidad, regla, ejemplo in FILAS:
        r = validar(regla)
        n = al_anio(regla) if r.se_puede_guardar else 0
        ok = r.se_puede_guardar and n > 0
        if not ok:
            malas += 1
        ws.cell(row=fila, column=1, value=dimension).font = Font(bold=True)
        ws.cell(row=fila, column=2, value=posibilidad)
        ws.cell(row=fila, column=3, value=n).alignment = Alignment(horizontal="right")
        ws.cell(row=fila, column=4, value=ejemplo)
        c = ws.cell(row=fila, column=5, value="sí" if ok else "NO")
        c.alignment = Alignment(horizontal="center")
        c.fill = PatternFill("solid",
                             start_color="DCFCE7" if ok else "FEE2E2")
        fila += 1

    for col, ancho in zip("ABCDE", (12, 40, 20, 52, 14)):
        ws.column_dimensions[col].width = ancho
    ws.freeze_panes = "A5"

    wb.save(destino)
    print(f"Escrito {destino}: {len(FILAS)} posibilidades, {malas} sin cubrir.")
    return 1 if malas else 0


def main() -> int:
    if "--excel" in sys.argv:
        return a_excel()

    print(f"\nLo que el sistema puede expresar — ejecutado contra el motor, {ANIO}\n")
    print(f"  {'':9} {'posibilidad':38} {'al año':>7}   ejemplo")
    print("  " + "─" * 100)
    dim_previa, malas = None, 0
    for dimension, posibilidad, regla, ejemplo in FILAS:
        r = validar(regla)
        n = al_anio(regla) if r.se_puede_guardar else 0
        if not r.se_puede_guardar or n == 0:
            malas += 1
            marca = "  ✗"
        else:
            marca = "   "
        etiqueta = dimension if dimension != dim_previa else ""
        dim_previa = dimension
        print(f"{marca}{etiqueta:9} {posibilidad:38} {n:7}   {ejemplo}")

    print()
    if malas:
        print(f"  {malas} fila(s) NO se pueden crear. El cuadro promete algo que el "
              f"sistema no hace.")
        return 1
    print(f"  Las {len(FILAS)} se pueden crear y generan vencimientos.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
