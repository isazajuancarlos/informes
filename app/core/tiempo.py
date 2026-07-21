"""Conversión de instantes guardados en UTC a la zona horaria del sistema.

# El problema que resuelve

Este sistema mezcla dos relojes, y la mezcla es histórica:

- Ocho columnas del modelo llevan `default=datetime.utcnow` — guardan **UTC sin
  zona** (naive).
- Cuarenta y cinco sitios del código usan `datetime.now()` — hora **local sin
  zona**.

Mientras cada dato se compare consigo mismo no pasa nada, y de hecho la ruta
crítica está bien: `fecha_limite_teorica` se construye desde fechas del
calendario local y se compara contra `datetime.now()` local, así que decidir
CUMPLIDO vs EXTEMPORÁNEO es correcto.

Donde sí rompía es al **mostrar** un valor UTC como si fuera local. El registro
de auditoría de seguridad —el que el propio modelo cita para la Ley 1581/2012 y
la ISO 27001 A.12.4— guardaba en UTC y se pintaba con `strftime` directo: en
Colombia, **cada evento aparecía cinco horas en el futuro**. Un rastro de
auditoría con las horas corridas no es un rastro incompleto, es uno que miente,
y encima en silencio.

# Por qué se arregla al MOSTRAR y no al guardar

Todas las filas existentes son UTC, sin excepción: la columna nunca se fijó a
mano. Convertir en la presentación las corrige **todas, retroactivamente**, sin
migración y sin dejar una discontinuidad entre "las de antes" y "las de después"
que alguien tendría que recordar para siempre.

Cambiar el almacenamiento habría arreglado solo lo nuevo y dejado lo viejo mal.
"""
from __future__ import annotations

from datetime import datetime, timezone
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError

from app.core.config import TIMEZONE

_FORMATO = "%Y-%m-%d %H:%M:%S"


def _zona() -> timezone | ZoneInfo:
    """La zona configurada, o UTC si el nombre no existe en el sistema.

    Se cae a UTC —y no a la hora local del servidor— a propósito: si la zona
    está mal escrita, mostrar UTC es visiblemente raro y alguien lo corrige;
    mostrar la del servidor parece correcto y nadie se entera nunca.
    """
    try:
        return ZoneInfo(TIMEZONE)
    except (ZoneInfoNotFoundError, ValueError):
        return timezone.utc


def utc_a_local(momento: datetime | None) -> datetime | None:
    """Interpreta `momento` como UTC y lo devuelve en la zona configurada.

    Acepta datetimes SIN zona (que es como los guarda SQLAlchemy con
    `default=datetime.utcnow`) y también con zona, por si algún día se migra.
    """
    if momento is None:
        return None
    if momento.tzinfo is None:
        momento = momento.replace(tzinfo=timezone.utc)
    return momento.astimezone(_zona())


def formatear_utc(momento: datetime | None, formato: str = _FORMATO) -> str:
    """`momento` (UTC) formateado en hora local. Cadena vacía si es None."""
    local = utc_a_local(momento)
    return local.strftime(formato) if local else ""
