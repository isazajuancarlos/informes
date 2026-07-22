"""Migración: añade la columna `firma` a `entregas_esperadas`.

Uso:
    python migrate_firma.py

Idempotente: se puede correr varias veces sin daño. Cada sentencia va en su
propia transacción para que un fallo no aborte las siguientes, igual que
`migrate_v4.py`.

Qué guarda esa columna: la firma post-cuántica (Ed25519 + ML-DSA-87, vía Quipu)
de los campos auditables de la entrega. Ver `app/services/signing_service.py`.

**No firma las entregas que ya existen, y es deliberado.** Firmar hoy una entrega
registrada hace seis meses produciría una firma con fecha de hoy sobre datos de
entonces: parecería probar algo que no probó nadie en su momento. Una firma que
certifica retroactivamente es peor que la ausencia de firma, porque la ausencia
es honesta. Las entregas anteriores quedan con `firma = NULL`, que la interfaz
muestra como «sin firma» y no como «inválida».
"""
import os
import sys

sys.path.append(os.path.dirname(os.path.abspath(__file__)))

from sqlalchemy import text

from app.core.database import engine

MIGRACIONES = [
    "ALTER TABLE entregas_esperadas ADD COLUMN IF NOT EXISTS firma TEXT",
]


def main() -> int:
    # OBSOLETA Y ROTA EN SQLITE. No se borra por si algún despliegue la invoca,
    # pero no debe ejecutarse: `ADD COLUMN IF NOT EXISTS` e `information_schema`
    # son de PostgreSQL, y en SQLite —el motor de los perfiles standalone y
    # cloud— fallan las dos. La columna nunca se creaba y la comprobación
    # tampoco podía correr: informaba «NO SE PUDO VERIFICAR» y devolvía 1 sin
    # que nadie mirara.
    #
    # Esto va en código y no en el docstring a propósito: un aviso que hay que
    # leer no impide nada.
    print("OBSOLETA: use `python migrar_esquema.py`, que funciona en SQLite y "
          "en PostgreSQL y verifica el resultado contra el esquema.")
    return 1

    aplicadas = 0
    fallos = 0
    for sql in MIGRACIONES:
        try:
            with engine.begin() as conn:
                conn.execute(text(sql))
            print(f"  OK    {sql[:70]}")
            aplicadas += 1
        except Exception as e:  # noqa: BLE001 — se informa y se sigue
            print(f"  FALLA {sql[:70]}\n        {e}")
            fallos += 1

    print(f"\n{aplicadas} sentencia(s) aplicada(s), {fallos} fallo(s).")

    # Comprobar el resultado en vez de fiarse de que el ALTER no lanzó: una
    # migración que "no falla" y no deja la columna es indistinguible de una que
    # funcionó, y eso es justo lo que no se puede permitir aquí.
    try:
        with engine.connect() as conn:
            existe = conn.execute(text(
                "SELECT 1 FROM information_schema.columns "
                "WHERE table_name = 'entregas_esperadas' AND column_name = 'firma'"
            )).first()
        if existe:
            print("VERIFICADO: la columna `firma` existe en entregas_esperadas.")
            return 0 if fallos == 0 else 1
        print("ERROR: la columna `firma` NO existe tras la migración.")
        return 1
    except Exception as e:  # noqa: BLE001
        print(f"NO SE PUDO VERIFICAR el resultado: {e}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
