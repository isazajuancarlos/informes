"""Congela lo que el motor Python produce, para que el de Rust lo reproduzca.

    python herramientas/congelar_oraculo.py

# Por qué existe

Portar el motor de periodicidad a otro lenguaje leyendo el código es un acto de
fe: la lógica sutil —el n-ésimo día hábil, el ajuste que separa nominal de
límite, «el último martes», la semana que cruza el mes— se reproduce «parecida»
y nadie se entera hasta que una entidad reclama una fecha.

Aquí no se traduce: se **mide**. Se ejecuta el motor de producción sobre los 80
patrones de una base real, se guardan las fechas EXACTAS que produce, y el motor
nuevo tiene que devolver las mismas. Byte a byte. Cualquier diferencia es un
fallo, no una conversación.

`herramientas/verificar_patrones.py` comprueba PROPIEDADES (cuántas al año, hacia
dónde se mueve el ajuste). Eso basta para detectar que el motor se rompió, pero
no para decidir si dos implementaciones distintas coinciden: dos motores pueden
cumplir las mismas propiedades y diferir en una fecha. Por eso el oráculo del
port guarda fechas, no propiedades.

# Los festivos van aparte, y es lo primero que hay que igualar

El motor Python deriva los festivos de la librería `holidays`, que a su vez los
deriva de la ley —incluida la Ley 51 de 1983, que corre varios al lunes
siguiente—. El motor de Rust no puede depender de ella, así que implementa la
ley. La única manera honesta de saber si la implementó bien es comparar los dos
calendarios día a día durante décadas, que es lo que se congela aquí.

Si algún día cambia la ley de festivos, este archivo se regenera y el motor de
Rust se pone rojo en la fecha exacta que cambió. Eso es lo que se le vende al
cliente como «actualización normativa».
"""
from __future__ import annotations

import json
import os
import sys
from datetime import date, timedelta
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(RAIZ))
os.environ.setdefault("DATABASE_URL", "sqlite:///:memory:")
os.environ.setdefault("DEPLOYMENT_PROFILE", "standalone")

from app.services.scheduler import GeneradorTareas as G  # noqa: E402
from app.services.calendar_engine import MotorReglasDinamicas  # noqa: E402
from herramientas.verificar_patrones import traducir  # noqa: E402

#: Rango de los festivos. Ancho a propósito: un calendario que solo coincide en
#: los años probados no sirve, porque las reglas se escriben una vez y corren
#: durante décadas.
FESTIVOS_DESDE, FESTIVOS_HASTA = 2020, 2045

#: Años sobre los que se generan los vencimientos de cada patrón. Cinco cubren
#: bisiesto, los dos comienzos de semana posibles del 1 de enero y suficientes
#: puentes de la Ley Emiliani.
VENC_DESDE, VENC_HASTA = 2026, 2030

DESTINO = RAIZ / "herramientas" / "oraculo"


def congelar_festivos() -> dict:
    motor = MotorReglasDinamicas()
    fechas = []
    dia = date(FESTIVOS_DESDE, 1, 1)
    fin = date(FESTIVOS_HASTA, 12, 31)
    while dia <= fin:
        if motor._es_festivo(dia):
            fechas.append(dia.isoformat())
        dia += timedelta(days=1)
    return {"desde": FESTIVOS_DESDE, "hasta": FESTIVOS_HASTA, "festivos": fechas}


def congelar_patrones(patrones: list) -> dict:
    salida, sin_traduccion = [], []
    for periodicidad, codigo in patrones:
        regla, motivo = traducir(periodicidad, codigo)
        if regla is None:
            # Un código que la notación de origen no permite traducir no es un
            # fallo del motor: se anota para que el port sepa que tampoco tiene
            # que traducirlo, y no lo invente.
            sin_traduccion.append({"periodicidad": periodicidad,
                                   "codigo": codigo, "motivo": motivo})
            continue

        vencimientos = []
        for anio in range(VENC_DESDE, VENC_HASTA + 1):
            dia, fin = date(anio, 1, 1), date(anio, 12, 31)
            while dia <= fin:
                for v in G.obtener_vencimientos_para_fecha(dia, regla):
                    vencimientos.append({
                        "nominal": v.nominal.isoformat(),
                        "limite": v.limite.isoformat(),
                        "ajuste": v.ajuste,
                        "hora": v.hora,
                    })
                dia += timedelta(days=1)

        salida.append({
            "periodicidad": periodicidad,
            "codigo": codigo,
            "motivo": motivo,
            "regla": regla,
            "vencimientos": vencimientos,
        })
    return {"desde": VENC_DESDE, "hasta": VENC_HASTA,
            "patrones": salida, "sin_traduccion": sin_traduccion}


def main() -> int:
    patrones = json.loads(
        (RAIZ / "herramientas" / "patrones_referencia.json").read_text("utf-8"))

    DESTINO.mkdir(parents=True, exist_ok=True)

    festivos = congelar_festivos()
    (DESTINO / "festivos_co.json").write_text(
        json.dumps(festivos, indent=1, ensure_ascii=False) + "\n", "utf-8")

    venc = congelar_patrones(patrones)
    (DESTINO / "vencimientos.json").write_text(
        json.dumps(venc, indent=1, ensure_ascii=False) + "\n", "utf-8")

    total = sum(len(p["vencimientos"]) for p in venc["patrones"])
    print(f"festivos    : {len(festivos['festivos'])} entre "
          f"{FESTIVOS_DESDE} y {FESTIVOS_HASTA}")
    print(f"patrones    : {len(venc['patrones'])} traducidos, "
          f"{len(venc['sin_traduccion'])} sin traducción (por la notación)")
    print(f"vencimientos: {total} fechas exactas entre {VENC_DESDE} y {VENC_HASTA}")

    if total == 0:
        print("El oráculo salió vacío: no mide nada.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
