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
    # Los campos del informe dejaron de ser columnas fijas y pasaron a ser
    # datos. `ambito` distingue el metadato del informe (FICHA) del dato que la
    # entrega lleva y se valida (CONTENIDO).
    ("meta_variables", "ambito", "VARCHAR(20)"),
    ("meta_variables", "opciones", "TEXT"),
    ("meta_variables", "orden", "INTEGER"),
    ("meta_variables", "activo", "BOOLEAN"),
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
            # El caso frecuente es una instalación nueva, donde las tablas aún
            # no existen. Decir «no se pudo leer el esquema: entregas_esperadas»
            # no ayuda a nadie a las once de la noche en el servidor de un
            # cliente; decir qué comando falta, sí.
            if tabla not in inspect(engine).get_table_names():
                print(f"ERROR: la tabla `{tabla}` no existe todavía.\n"
                      f"       En una instalación nueva, cree primero el "
                      f"esquema:\n\n"
                      f"           python -m app.init_db\n\n"
                      f"       y vuelva a ejecutar esta migración.")
            else:
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
    return rellenar_valores()


#: Columnas nuevas que NO pueden quedarse en NULL, con lo que deben valer en las
#: filas que ya existían.
#:
#: Un `default=` del modelo solo actúa al insertar por el ORM: las filas
#: anteriores a la columna se quedan en NULL, y un `ambito` nulo no es ninguno de
#: los dos ámbitos — el campo no aparecería en ninguna de las dos secciones y
#: nadie sabría por qué. Añadir la columna sin rellenarla es dejar el trabajo a
#: medias de la manera que no se nota.
RELLENOS = [
    ("meta_variables", "ambito", "'CONTENIDO'"),
    ("meta_variables", "activo", "1"),
    ("meta_variables", "orden", "0"),
]


def rellenar_valores() -> int:
    from sqlalchemy import text as _text

    hechos = 0
    for tabla, columna, valor in RELLENOS:
        sql = f"UPDATE {tabla} SET {columna} = {valor} WHERE {columna} IS NULL"
        try:
            with engine.begin() as conn:
                resultado = conn.execute(_text(sql))
            if resultado.rowcount:
                print(f"  RELLENADAS {resultado.rowcount} fila(s): "
                      f"{tabla}.{columna} = {valor}")
                hechos += resultado.rowcount
        except Exception as e:  # noqa: BLE001
            print(f"  FALLA      {sql}\n             {e}")
            return 1

    # Se VERIFICA que no quede ningún nulo, por la misma razón que se releen las
    # columnas: que el UPDATE no lanzara excepción no prueba que no quede nada.
    for tabla, columna, _ in RELLENOS:
        with engine.begin() as conn:
            quedan = conn.execute(_text(
                f"SELECT COUNT(*) FROM {tabla} WHERE {columna} IS NULL")).scalar()
        if quedan:
            print(f"\nERROR: quedan {quedan} fila(s) con {tabla}.{columna} en nulo.")
            return 1

    print(f"VERIFICADO contra los datos: sin valores nulos"
          + (f" ({hechos} fila(s) rellenadas)." if hechos else "."))
    return 0


if __name__ == "__main__":
    sys.exit(main())
