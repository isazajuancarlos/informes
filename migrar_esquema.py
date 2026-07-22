"""Migración de esquema: añade columnas que faltan en instalaciones existentes.

Uso:
    python migrar_esquema.py

Idempotente: se puede correr las veces que haga falta. Devuelve 0 solo si al
terminar **todas** las columnas existen de verdad, comprobadas contra el motor.

# Por qué reemplaza a `migrate_firma.py`

Aquella migración usaba `ADD COLUMN IF NOT EXISTS` e `information_schema`, que
son de PostgreSQL. **En SQLite fallan las dos**, y SQLite es el motor de los
perfiles `standalone` y `cloud` — es decir, el de la edición que se entrega
instalada. La migración de la firma nunca se aplicó ahí, y su comprobación
tampoco podía correr: informaba «NO SE PUDO VERIFICAR» sin que nadie mirara.

Es el mismo patrón que ya costó caro en otros sitios: **un verificador que no
puede ponerse rojo no informa de nada**. Aquí la detección de columnas existentes
se hace con el inspector de SQLAlchemy, que habla los dos dialectos, y el
resultado se comprueba releyendo el esquema.

# Qué NO hace

No rellena datos. Las columnas nuevas quedan en NULL en las filas antiguas, y eso
es deliberado:

  - `firma`: firmar hoy una entrega de hace seis meses produciría una firma con
    fecha de hoy sobre datos de entonces. Certificaría retroactivamente algo que
    nadie certificó. NULL se muestra como «sin firma», que es la verdad.
  - `fecha_vencimiento_nominal`: copiarle el valor de `fecha_limite_teorica`
    haría creer que la distinción entre día nominal y día límite se aplicó a esas
    entregas. No se aplicó. Un dato inventado no se distingue de uno real, y esa
    es exactamente la diferencia que estas columnas existen para registrar.
"""
import os
import sys

sys.path.append(os.path.dirname(os.path.abspath(__file__)))

from sqlalchemy import inspect, text

from app.core.database import engine

#: (tabla, columna, tipo SQL). El tipo se escribe en la forma que entienden
#: tanto SQLite como PostgreSQL.
COLUMNAS = [
    ("entregas_esperadas", "firma", "TEXT"),
    ("entregas_esperadas", "fecha_vencimiento_nominal", "TIMESTAMP"),
    ("entregas_esperadas", "tipo_ajuste", "VARCHAR(20)"),
]


def columnas_existentes(tabla):
    """Las columnas que la tabla tiene AHORA, según el motor.

    Se pregunta al inspector en vez de a un catálogo concreto: lo que hay que
    saber es si la columna existe, no cómo la nombra cada motor.
    """
    return {c["name"] for c in inspect(engine).get_columns(tabla)}


def main() -> int:
    tablas = {t for t, _, _ in COLUMNAS}
    presentes = {}
    for tabla in tablas:
        try:
            presentes[tabla] = columnas_existentes(tabla)
        except Exception as e:  # noqa: BLE001
            print(f"ERROR: no se pudo leer el esquema de `{tabla}`: {e}")
            return 1

    pendientes = [(t, c, tipo) for t, c, tipo in COLUMNAS
                  if c not in presentes.get(t, set())]

    if not pendientes:
        print("Nada que hacer: todas las columnas ya existen.")
    for tabla, columna, tipo in pendientes:
        sql = f"ALTER TABLE {tabla} ADD COLUMN {columna} {tipo}"
        try:
            with engine.begin() as conn:
                conn.execute(text(sql))
            print(f"  APLICADA  {sql}")
        except Exception as e:  # noqa: BLE001 — se informa y se sigue
            print(f"  FALLA     {sql}\n            {e}")

    # Verificación: se relee el esquema. Que el ALTER no lanzara excepción no
    # prueba que la columna esté; comprobarlo cuesta una consulta.
    faltan = []
    for tabla in tablas:
        try:
            actuales = columnas_existentes(tabla)
        except Exception as e:  # noqa: BLE001
            print(f"NO SE PUDO VERIFICAR `{tabla}`: {e}")
            return 1
        faltan += [f"{tabla}.{c}" for t, c, _ in COLUMNAS
                   if t == tabla and c not in actuales]

    if faltan:
        print("\nERROR: tras la migración siguen faltando: " + ", ".join(faltan))
        return 1

    print(f"\nVERIFICADO contra el esquema: las {len(COLUMNAS)} columnas existen.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
