#!/usr/bin/env python3
"""Migra el flujo de cuatro mecanismos a uno: `participacion_informe`.

ANTES
  asignaciones_informe_usuario  partes internas, rol en `tipo_responsabilidad`
  rel_informe_solicitantes      partes externas que consumen (rol implícito)
  rel_informe_destinatarios     partes externas que reciben  (rol implícito)
  cat_informes.tipo_flujo       resumen almacenado, capaz de contradecir a las tres

DESPUÉS
  participacion_informe(informe_id, usuario_id | stakeholder_id, rol, ...)
  y `tipo_flujo` calculado a partir de las partes.

Es idempotente: se puede correr varias veces sin duplicar. NO borra las tablas
viejas — las deja intactas para poder comparar y volver atrás. Cuando la
migración esté verificada en producción, se eliminan aparte.

    python migrar_flujo_unificado.py            # migra
    python migrar_flujo_unificado.py --verificar # solo informa, no escribe
"""
import argparse
import sys

from sqlalchemy import inspect, text

from app.core.database import engine, SessionLocal
from app.models.meta_tables import (
    ROL_DESTINO, ROL_EJECUTA, ROL_ORIGEN, ROL_SUPERVISA, ROL_USUARIO,
)

# Cómo se llamaba el rol antes -> cómo se llama ahora.
# ENVIA/RECIBE describían la dirección; ORIGEN/DESTINO describen el papel en el
# flujo, que es lo mismo dicho sin ambigüedad.
MAPA_RESPONSABILIDAD = {
    "EJECUTA": ROL_EJECUTA,
    "ENVIA": ROL_ORIGEN,
    "RECIBE": ROL_DESTINO,
    "SUPERVISA": ROL_SUPERVISA,
}
ROL_POR_DEFECTO = ROL_EJECUTA


def _tablas_existentes():
    return set(inspect(engine).get_table_names())


def _agregar_columnas_de_alerta(solo_verificar: bool):
    """Sella de aviso enviado, para que las alertas no se repitan.

    Sin estas dos columnas el servicio reenviaba el mismo correo de
    incumplimiento en cada corrida —cada 6 h, indefinidamente—, porque su
    condición era "vencida y pendiente", que no deja de cumplirse nunca.
    """
    columnas = {c["name"] for c in inspect(engine).get_columns("entregas_esperadas")}
    faltantes = [c for c in ("alerta_proximidad_en", "alerta_incumplimiento_en")
                 if c not in columnas]
    if not faltantes:
        return
    if solo_verificar:
        print(f"ℹ️  Se agregarían columnas a entregas_esperadas: {', '.join(faltantes)}")
        return
    with engine.begin() as conn:
        for col in faltantes:
            conn.execute(text(f"ALTER TABLE entregas_esperadas ADD COLUMN {col} TIMESTAMP"))
    print(f"✅ Columnas agregadas a entregas_esperadas: {', '.join(faltantes)}")


def migrar(solo_verificar: bool = False) -> int:
    tablas = _tablas_existentes()

    if "participacion_informe" not in tablas:
        print("✋ Falta la tabla participacion_informe. Corre primero la creación "
              "de esquema (reset_total.py o el arranque de la app).")
        return 1

    _agregar_columnas_de_alerta(solo_verificar)

    insertadas = 0
    omitidas = 0
    huerfanas = 0

    with SessionLocal() as db:
        def ya_existe(informe_id, usuario_id, stakeholder_id, rol) -> bool:
            fila = db.execute(text(
                "SELECT 1 FROM participacion_informe WHERE informe_id = :i AND rol = :r "
                "AND usuario_id IS :u AND stakeholder_id IS :s"
            ).bindparams(i=informe_id, r=rol, u=usuario_id, s=stakeholder_id)).first()
            return fila is not None

        def insertar(informe_id, usuario_id, stakeholder_id, rol, es_principal=True, orden=0):
            nonlocal insertadas, omitidas
            if ya_existe(informe_id, usuario_id, stakeholder_id, rol):
                omitidas += 1
                return
            # También en modo --verificar se INSERTA de verdad: es la única
            # forma de que el contraste de flujos de abajo sea real. Lo que
            # distingue al simulacro es el rollback del final, no saltarse la
            # escritura. Contar sin escribir hacía que el simulacro reportara
            # discrepancias distintas —e inventadas— de las de la corrida real.
            db.execute(text(
                "INSERT INTO participacion_informe "
                "(informe_id, usuario_id, stakeholder_id, rol, es_principal, orden) "
                "VALUES (:i, :u, :s, :r, :p, :o)"
            ).bindparams(i=informe_id, u=usuario_id, s=stakeholder_id, r=rol,
                         p=bool(es_principal), o=orden or 0))
            insertadas += 1

        # --- 1. Partes internas -------------------------------------------
        if "asignaciones_informe_usuario" in tablas:
            for fila in db.execute(text(
                "SELECT informe_id, usuario_id, tipo_responsabilidad, es_principal, orden "
                "FROM asignaciones_informe_usuario"
            )).mappings():
                if not fila["informe_id"] or not fila["usuario_id"]:
                    huerfanas += 1
                    continue
                rol = MAPA_RESPONSABILIDAD.get(
                    (fila["tipo_responsabilidad"] or "").upper(), ROL_POR_DEFECTO)
                insertar(fila["informe_id"], fila["usuario_id"], None, rol,
                         fila["es_principal"], fila["orden"])
        else:
            print("ℹ️  No existe asignaciones_informe_usuario (base nueva).")

        # --- 2. Partes externas que CONSUMEN ------------------------------
        if "rel_informe_solicitantes" in tablas:
            for fila in db.execute(text(
                "SELECT informe_id, stakeholder_id FROM rel_informe_solicitantes"
            )).mappings():
                if not fila["informe_id"] or not fila["stakeholder_id"]:
                    huerfanas += 1
                    continue
                insertar(fila["informe_id"], None, fila["stakeholder_id"], ROL_USUARIO)

        # --- 3. Partes externas que RECIBEN -------------------------------
        if "rel_informe_destinatarios" in tablas:
            for fila in db.execute(text(
                "SELECT informe_id, stakeholder_id FROM rel_informe_destinatarios"
            )).mappings():
                if not fila["informe_id"] or not fila["stakeholder_id"]:
                    huerfanas += 1
                    continue
                insertar(fila["informe_id"], None, fila["stakeholder_id"], ROL_DESTINO)

        verbo = "se insertarían" if solo_verificar else "insertadas"
        print(f"✅ Participaciones {verbo}: {insertadas}")
        print(f"   ya presentes (omitidas): {omitidas}")
        if huerfanas:
            print(f"   ⚠️  filas huérfanas descartadas: {huerfanas}")

        # --- 4. Contraste del flujo declarado contra el derivado ----------
        # El motivo de todo el cambio: cuántos informes tenían un tipo_flujo
        # que sus propias partes desmentían.
        #
        # OJO: esto va DENTRO de la transacción, antes del commit/rollback. Si
        # se calculara después, en modo --verificar el rollback ya habría
        # borrado las participaciones y todos los informes derivarían
        # ENVIO_INTERNO: el simulacro reportaría discrepancias inventadas y
        # distintas de las de la corrida real.
        columnas = {c["name"] for c in inspect(engine).get_columns("cat_informes")}
        if "tipo_flujo" in columnas:
            _reportar_contradicciones(db)

        if solo_verificar:
            db.rollback()
        else:
            db.commit()

    return 0


def _reportar_contradicciones(db):
    from app.models.meta_tables import Informe

    declarados = dict(db.execute(text("SELECT id, tipo_flujo FROM cat_informes")).all())
    db.expire_all()  # que las propiedades vean las participaciones recién escritas
    discrepantes = []
    for inf in db.query(Informe).all():
        antes = declarados.get(inf.id) or "ENVIO_INTERNO"
        ahora = inf.tipo_flujo
        if antes != ahora:
            discrepantes.append((inf.id, inf.nombre, antes, ahora))

    if not discrepantes:
        print("   El flujo declarado coincidía con el derivado en todos los informes.")
        return

    print(f"\n⚠️  {len(discrepantes)} informe(s) tenían un tipo_flujo que sus partes "
          f"desmienten. Gana el derivado:")
    for id_, nombre, antes, ahora in discrepantes[:20]:
        print(f"   #{id_} {nombre[:44]:44} {antes} → {ahora}")
    if len(discrepantes) > 20:
        print(f"   … y {len(discrepantes) - 20} más")


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--verificar", action="store_true",
                    help="no escribe: solo informa qué haría")
    args = ap.parse_args()
    sys.exit(migrar(solo_verificar=args.verificar))
