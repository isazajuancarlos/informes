"""Traduce la notación de una base real y comprueba que el motor la cumple.

    python herramientas/verificar_patrones.py patrones.json

# Por qué existe

Se había medido que los patrones de una base real de 399 informes eran
«expresables», clasificando los códigos con expresiones regulares. Eso mide la
FORMA del código, no lo que el motor hace con él: dice que la traducción existe,
no que sea correcta.

Es la misma diferencia que costó cara el resto del día —un verificador que mira
la etiqueta en vez del contenido— así que aquí se traduce cada código a una regla
de verdad, se ejecuta en el motor de producción y se comprueba el RESULTADO
contra lo que el propio código declara:

  - la periodicidad declarada en su primera columna fija cuántos vencimientos
    debe haber al año (mensual 12, trimestral 4, anual 1…);
  - el modificador `<`, `>` o `=` fija hacia dónde puede moverse la entrega.

Ninguna de esas dos comprobaciones sale del motor, así que no puede pasar por la
razón equivocada.

# La notación

    A11<31   (A)nual, mes 11, día 31 → último, ajuste hacia atrás
    M<15     (M)ensual, día 15, ajuste hacia atrás
    MH>5     (M)ensual, (H)ábil: 5º día HÁBIL
    MD<5     (M)ensual, (D)ías: primeros 5 días
    T1<5     (T)rimestral, mes 1 tras el trimestre, día 5
    S>35     (S)emanal, días 3 y 5 de la semana (miércoles y viernes)
    S=2      (S)emanal, martes, SIN ajuste aunque sea festivo
    MUM      (M)ensual, (U)ltimo (M)artes
    BM>7-20  (B)i(M)ensual, días 7 y 20
    FECHA    sin patrón: lista de fechas

`X` significa «indefinido» y no altera el cálculo.
"""
from __future__ import annotations

import json
import os
import re
import sys
from datetime import date, timedelta

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.environ.setdefault("DATABASE_URL", "sqlite:///:memory:")

from app.services.scheduler import GeneradorTareas as G  # noqa: E402
from app.services.validar_regla import validar  # noqa: E402

TODOS = list(range(1, 13))

#: Cuántos vencimientos al año implica cada periodicidad declarada. Sale de la
#: palabra, no del motor.
AL_ANIO = {
    "DIARIO": None, "DIARIA": None,   # depende de los días hábiles
    "SEMANAL": None,                  # 52 o 53, y puede tener varios días
    "2 VECES POR SEMANA": None,
    "BIMENSUAL": 24, "QUINCENAL": 24,  # dos veces al mes
    "MENSUAL": 12, "MENUSAL": 12,
    "BIMESTRAL": 6,
    "TRIMESTRAL": 4,
    "SEMESTRAL": 2,
    "ANUAL": 1,
    "FECHA": None,
}

#: Frecuencias que no fijan un número: se comprueba que genere ALGO y que
#: respete la dirección del ajuste, pero no cuántos.
#:
#: Antes, una frecuencia desconocida caía a «MENSUAL» y se le exigían 12 al año.
#: Eso marcó como FALLA cuatro informes cuyo motor estaba perfecto —una
#: quincenal genera 24 y dos veces por semana genera 104— y el fallo era del
#: banco de pruebas, no del sistema. Medir con la vara equivocada es peor que no
#: medir: sale un número creíble y falso.
SIN_NUMERO_FIJO = None

#: Meses de entrega según la periodicidad, cuando el código no los da.
MESES = {"MENSUAL": TODOS, "MENUSAL": TODOS, "BIMENSUAL": TODOS,
         "QUINCENAL": TODOS, "DIARIA": TODOS, "2 VECES POR SEMANA": TODOS,
         "BIMESTRAL": [1, 3, 5, 7, 9, 11], "TRIMESTRAL": [3, 6, 9, 12],
         "SEMESTRAL": [6, 12], "ANUAL": [12]}


def traducir(periodicidad: str, codigo: str) -> tuple[dict | None, str]:
    """(regla, motivo). `regla` es None si el código no se sabe traducir."""
    c = codigo.strip().upper().replace(" ", "")
    p = periodicidad.strip().upper()
    inicio = {"fecha_inicio": "2026-01-01"}

    if c == "FECHA":
        return ({"modo": "FECHAS", "fechas": ["2026-04-15", "2026-09-20"], **inicio},
                "lista de fechas")
    if c == "MUM":
        return ({"modo": "ESPECIFICO", "meses": TODOS, "dia_semana_mes": 1,
                 "semana_del_mes": -1, **inicio}, "último martes")

    if re.match(r"^M\dD[<>=]\d+$", c):
        # `M3D<7` = «tres días al mes, el primero el 7». Los otros dos (10 y 20)
        # NO están en el código: viven en la descripción en texto libre. El
        # motor SÍ admite la forma —`dias_corte`— pero de este código no se
        # pueden sacar los días, así que no es un hueco del sistema sino un
        # límite de la notación de origen.
        return (None, "el código no lleva los días: están solo en la descripción")

    m = re.match(r"^([A-Z]+?)(X?)(\d*)([<>=])(\d{1,2})(?:-(\d{1,2}))?$", c)
    if not m:
        return (None, f"no se reconoce la forma del código «{codigo}»")
    letras, _x, numero, signo, dia, dia2 = m.groups()
    dia, dia2 = int(dia), int(dia2) if dia2 else None

    ajuste = {}
    if signo == "<":
        ajuste = {"ajuste_habil_anterior": True, "mover_habil_anterior": True}
    elif signo == ">":
        ajuste = {"ajuste_habil": True, "mover_habil": True}

    # SEMANAL: el número es el día de la SEMANA (1=lunes … 5=viernes), y dos
    # dígitos son dos días. No es un día del mes: ese es justo el error que la
    # línea lógica del asistente existe para impedir.
    if p in ("SEMANAL", "2 VECES POR SEMANA") and letras.startswith("S") \
            and p != "SEMESTRAL":
        dias = [int(d) - 1 for d in str(dia)] if dia > 6 else [dia - 1]
        if "H" in letras:
            # El n-ésimo día hábil DE LA SEMANA. Ya es hábil por construcción,
            # así que no lleva ajuste.
            return ({"modo": "SEMANAL", "dia_habil_semana": dia, **inicio},
                    f"{dia}º día hábil de la semana")
        return ({"modo": "SEMANAL", "dias_semana": dias, **ajuste, **inicio},
                f"días de la semana {dias}")

    meses = MESES.get(p, TODOS)
    base = {"modo": "ESPECIFICO", "meses": meses, **ajuste, **inicio}

    if "H" in letras:                       # n-ésimo día hábil
        base.pop("ajuste_habil", None); base.pop("ajuste_habil_anterior", None)
        base.pop("mover_habil", None); base.pop("mover_habil_anterior", None)
        return ({**base, "dia_habil": dia}, f"{dia}º día hábil")
    if dia2 is not None:                    # dos días de corte
        return ({**base, "dias_corte": [dia, dia2]}, f"días {dia} y {dia2}")
    if dia == 31:                           # «31» en esta notación es «último»
        return ({**base, "es_ultimo_dia": True}, "último día del mes")
    return ({**base, "dia_corte": dia}, f"día {dia}")


def vencimientos(regla, anio=2026):
    salida, dia = [], date(anio, 1, 1)
    while dia <= date(anio, 12, 31):
        salida += G.obtener_vencimientos_para_fecha(dia, regla)
        dia += timedelta(days=1)
    return salida


def comprobar(periodicidad, codigo):
    """(estado, detalle). estado: OK | SIN_TRADUCCION | FALLA."""
    regla, motivo = traducir(periodicidad, codigo)
    if regla is None:
        return ("SIN_TRADUCCION", motivo)

    r = validar(regla)
    if not r.se_puede_guardar:
        return ("FALLA", f"el validador la rechaza: {r.errores[0][:70]}")

    venc = vencimientos(regla)
    if not venc:
        return ("FALLA", "no genera ningún vencimiento")

    # Una frecuencia que no se conoce NO se sustituye por otra: se deja sin
    # número esperado. Rellenarla con la mensual fue lo que produjo cuatro
    # fallos inventados.
    esperados = AL_ANIO.get(periodicidad.strip().upper(), SIN_NUMERO_FIJO)
    if regla.get("modo") == "FECHAS":
        # `FECHA` significa «sin patrón»: las fechas viven en otra hoja del
        # libro, no en el código. Exigirle el número que implica la frecuencia
        # declarada es medir con la vara de otra cosa.
        esperados = SIN_NUMERO_FIJO
    if esperados is not None and len(venc) != esperados:
        return ("FALLA", f"{len(venc)} vencimientos al año, se esperaban {esperados}")

    # Las formas SEMANALES pasaban por aquí SIN COMPROBAR NADA: no se les fija
    # cuántas al año —pueden ser 52 o 53, con uno o varios días marcados— y sin
    # ajuste tampoco hay dirección que mirar. Lo detectó una mutación: contar
    # días corridos en vez de hábiles no ponía roja esta herramienta.
    #
    # Un verificador con un punto ciego es peor que no tenerlo, porque su verde
    # se lee como comprobación.
    if regla.get("modo") == "SEMANAL":
        if regla.get("dia_habil_semana"):
            from app.services.calendar_engine import MotorReglasDinamicas
            motor = MotorReglasDinamicas()
            for v in venc:
                if not motor._es_habil(v.nominal):
                    return ("FALLA",
                            f"«H» exige día hábil y {v.nominal} no lo es")
            semanas = {v.nominal.isocalendar()[:2] for v in venc}
            if len(semanas) != len(venc):
                return ("FALLA", "más de un vencimiento en la misma semana")
        else:
            marcados = set(regla.get("dias_semana", []))
            for v in venc:
                if v.nominal.weekday() not in marcados:
                    return ("FALLA",
                            f"{v.nominal} cae en un día de la semana no marcado")
        if len(venc) < 40:
            return ("FALLA", f"solo {len(venc)} vencimientos en un año semanal")

    # La dirección del ajuste tiene que respetar el modificador del código.
    signo = next((s for s in "<>=" if s in codigo), None)
    for v in venc:
        if not v.se_movio:
            continue
        if signo == "<" and v.limite > v.nominal:
            return ("FALLA", f"«<» debe ir hacia atrás y {v.nominal}→{v.limite}")
        if signo == ">" and v.limite < v.nominal:
            return ("FALLA", f"«>» debe ir hacia delante y {v.nominal}→{v.limite}")
        if signo == "=":
            return ("FALLA", f"«=» no debe moverse y {v.nominal}→{v.limite}")
    return ("OK", f"{motivo}, {len(venc)} al año")


def main() -> int:
    ruta = sys.argv[1] if len(sys.argv) > 1 else "patrones.json"
    patrones = json.load(open(ruta, encoding="utf-8"))

    conteo = {"OK": 0, "SIN_TRADUCCION": 0, "FALLA": 0}
    problemas = []
    for periodicidad, codigo in patrones:
        estado, detalle = comprobar(periodicidad, codigo)
        conteo[estado] += 1
        if estado != "OK":
            problemas.append((estado, periodicidad, codigo, detalle))

    total = len(patrones)
    print(f"{total} patrones de una base real, ejecutados contra el motor:\n")
    print(f"  CUMPLEN            {conteo['OK']:3}  ({100*conteo['OK']/total:.1f}%)")
    print(f"  sin traducción     {conteo['SIN_TRADUCCION']:3}")
    print(f"  FALLAN             {conteo['FALLA']:3}")
    if problemas:
        print()
        for estado, p, c, d in problemas:
            print(f"  {estado:15} {p:12} {c:10} {d}")

    # Solo un FALLA es un fallo del sistema: «sin traducción» dice que esta
    # herramienta no sabe leer ese código, que es otra cosa y hay que mirarla
    # aparte en vez de contarla como error del motor.
    return 1 if conteo["FALLA"] else 0


if __name__ == "__main__":
    sys.exit(main())
