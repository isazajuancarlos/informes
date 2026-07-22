"""Mueve los siete campos fijos del informe al mecanismo de campos definibles.

# Por qué

`Informe` tenía siete columnas copiadas del Excel de un cliente concreto:
subsistema, aplicativo de origen, cargo autorizador, URL y ruta de plantilla,
justificación normativa y acuerdo de confidencialidad. Eso convertía el ejemplo
en la especificación — otra entidad clasifica sus informes de otra manera y no
tenía dónde ponerlo — y añadir un campo exigía tocar el modelo, la migración y
la pantalla.

Ahora los campos son datos (`MetaVariable` con ámbito `FICHA`). Este script:

  1. crea las definiciones equivalentes, si no existen;
  2. **copia los valores** de las columnas viejas a `ConfiguracionInformeVariable`;
  3. verifica releyendo, y dice cuántos movió.

# Qué NO hace

No borra las columnas. Quedan inertes: nadie las lee ni las escribe ya —el
formulario dejó de hacerlo— y borrarlas en SQLite exige reconstruir la tabla,
que es un riesgo sin ninguna ganancia. La verificación de abajo comprueba lo
que importa: que ningún valor se haya quedado atrás.

Es **idempotente**: correrlo dos veces no duplica nada.

    python migrar_campos_ficha.py
"""
from __future__ import annotations

import os
import sys

sys.path.append(os.path.dirname(os.path.abspath(__file__)))

# NO se fija aquí ningún `DATABASE_URL` por defecto. Lo hacía, y con eso se
# apuntaba a `./informes.db` mientras la instalación real usaba la ruta del
# `.env`: el script encontraba una base vacía —o la creaba— y reventaba. El
# mismo defecto del valor de relleno que este producto existe para evitar,
# cometido en el script que mueve los datos.
#
# La base la decide `app.core.config`, que es quien lee el `.env`. Igual que
# `migrar_esquema.py`.
from app.core.database import get_db_session  # noqa: E402
from app.models.meta_tables import (  # noqa: E402
    AMBITO_FICHA, ConfiguracionInformeVariable, Informe, MetaVariable)

#: (columna del informe, nombre interno, etiqueta, tipo, ayuda)
#:
#: La etiqueta es la que ya tenía en pantalla: quien abra un informe migrado
#: tiene que reconocer sus campos, no encontrarse unos nombres nuevos.
CAMPOS = [
    ("subsistema", "subsistema", "Subsistema", "TEXTO",
     "Área o subsistema al que pertenece el informe."),
    ("aplicativo_origen", "aplicativo_origen", "Aplicativo/Herramienta Origen",
     "TEXTO", "De qué sistema sale la información."),
    ("url_plantilla", "url_plantilla", "URL Plantilla", "TEXTO",
     "Dirección web de la plantilla del informe."),
    ("ruta_plantilla", "ruta_plantilla", "Ruta Plantilla", "TEXTO",
     "Ruta en disco de la plantilla."),
    ("justificacion_normatividad", "justificacion_normatividad",
     "Justificación / Normatividad", "TEXTO_LARGO",
     "La norma o el contrato que obliga a este informe."),
    ("tiene_acuerdo_confidencialidad", "acuerdo_confidencialidad",
     "Requiere Acuerdo de Confidencialidad", "BOOLEANO",
     "Si la entrega exige acuerdo de confidencialidad firmado."),
    ("cargo_autorizador", "cargo_autorizador", "Cargo Autorizador", "TEXTO",
     "Cargo de quien autoriza la entrega."),
]


def _texto(valor):
    """El valor de la columna, como cadena. `None` si no hay nada que mover.

    `False` en una casilla NO es «no hay dato»: es un dato, y es «no». Tratarlo
    como vacío perdería la diferencia entre «no requiere acuerdo» y «nadie lo
    ha contestado», que es justo la distinción que este sistema existe para no
    perder.
    """
    if valor is None:
        return None
    if isinstance(valor, bool):
        return "Sí" if valor else "No"
    texto = str(valor).strip()
    return texto or None


def main() -> int:
    creados = movidos = 0
    with get_db_session() as db:
        definiciones = {}
        for columna, interno, etiqueta, tipo, ayuda in CAMPOS:
            campo = db.query(MetaVariable).filter_by(nombre_interno=interno).first()
            if campo is None:
                campo = MetaVariable(
                    nombre_interno=interno, etiqueta_visual=etiqueta,
                    tipo_dato=tipo, ambito=AMBITO_FICHA, descripcion=ayuda,
                    es_obligatorio=False, activo=True, orden=0)
                db.add(campo)
                db.flush()
                creados += 1
            else:
                # Puede existir de una corrida anterior con otro ámbito.
                campo.ambito = AMBITO_FICHA
            definiciones[columna] = campo.id
        db.commit()

        for inf in db.query(Informe).all():
            for columna, *_ in CAMPOS:
                valor = _texto(getattr(inf, columna, None))
                if valor is None:
                    continue
                variable_id = definiciones[columna]
                ya = db.query(ConfiguracionInformeVariable).filter_by(
                    informe_id=inf.id, variable_id=variable_id).first()
                if ya:
                    continue  # idempotente: no se pisa lo que ya se movió
                db.add(ConfiguracionInformeVariable(
                    informe_id=inf.id, variable_id=variable_id,
                    valor_parametro=valor[:255]))
                movidos += 1
        db.commit()

    print(f"Campos de ficha creados: {creados}")
    print(f"Valores movidos:         {movidos}")

    # VERIFICACIÓN: se relee. Que no hubiera excepción no prueba que los valores
    # estén donde deben; comprobarlo cuesta una consulta.
    with get_db_session() as db:
        ids = {c.nombre_interno: c.id for c in
               db.query(MetaVariable).filter_by(ambito=AMBITO_FICHA).all()}
        pendientes = []
        for inf in db.query(Informe).all():
            for columna, interno, *_ in CAMPOS:
                if _texto(getattr(inf, columna, None)) is None:
                    continue
                if interno not in ids:
                    pendientes.append(f"{inf.id}/{interno}")
                    continue
                if not db.query(ConfiguracionInformeVariable).filter_by(
                        informe_id=inf.id, variable_id=ids[interno]).first():
                    pendientes.append(f"{inf.id}/{interno}")
    if pendientes:
        print(f"\nERROR: {len(pendientes)} valor(es) no llegaron: "
              + ", ".join(pendientes[:8]) + ("…" if len(pendientes) > 8 else ""))
        return 1

    print("VERIFICADO releyendo: ningún valor se quedó en las columnas viejas.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
