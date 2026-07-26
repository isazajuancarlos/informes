"""Conversión de instantes guardados en UTC a la zona horaria del sistema.

# El problema que resuelve

Este sistema mezcla dos relojes, y la mezcla es histórica:

- Seis columnas del modelo guardan **UTC sin zona** (naive).
- Una, `EntregaEsperada.fecha_programada`, guarda hora **local** — aunque su
  default declarase UTC, porque los cinco sitios que crean entregas la fijan a
  mano en local y el default nunca llegaba a usarse.
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


def ahora_utc() -> datetime:
    """Instante actual en UTC, **sin zona**, para las columnas que guardan UTC.

    Sin zona a propósito, y esto no es pereza: todas las filas que ya existen son
    naive, y las columnas son `DateTime` sin `timezone=True`. Devolver un
    datetime CON zona haría que un valor recién creado no se pudiera comparar con
    uno leído de la base —`TypeError: can't compare offset-naive and offset-aware`—
    y el fallo no aparecería en el momento de guardar, sino más tarde y lejos.

    Sustituye a `datetime.utcnow()`, deprecado desde Python 3.12 y marcado para
    eliminación. El comportamiento es idéntico; lo que cambia es que ya no
    depende de una función que va a desaparecer.

    Si algún día se migra a datetimes con zona, **este es el sitio**: cambiar
    aquí, migrar las filas y revisar las comparaciones contra los `datetime.now()`
    locales. Por eso existe la función en vez de repetir la expresión.
    """
    return datetime.now(timezone.utc).replace(tzinfo=None)


def ahora_local() -> datetime:
    """Instante actual en hora local, **sin zona**.

    Es `datetime.now()` con nombre. Existe para que el modelo diga con qué reloj
    trabaja cada columna en vez de dejarlo a la arqueología: en un sistema con
    dos relojes, `default=ahora_local` frente a `default=ahora_utc` es la única
    documentación que nadie puede dejar desactualizada.
    """
    return datetime.now()


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
    `default=ahora_utc`) y también con zona, por si algún día se migra.
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


def describir_vencimiento(entrega) -> str:
    """Las dos fechas de una entrega, en una línea.

    El día de vencimiento y el día de entrega son cosas distintas, y mostrar
    solo una deja al responsable sin saber lo único que necesita: si entregar el
    día que ve en pantalla es cumplir o llegar tarde.

        Vence 2026-01-01 · plazo hasta 2026-01-02     (el 2 NO es tarde)
        Vence 2026-01-01 · entregar antes de 2025-12-31 (el 1 ya es tarde)

    Las entregas anteriores a la distinción no tienen día nominal guardado. Se
    muestra solo el límite, sin inventarle uno: un valor de relleno haría creer
    que la distinción se aplicó cuando no se aplicó.
    """
    if entrega.fecha_limite_teorica is None:
        return "Sin límite"

    limite = entrega.fecha_limite_teorica.strftime("%Y-%m-%d %H:%M")
    nominal_dt = getattr(entrega, "fecha_vencimiento_nominal", None)
    if nominal_dt is None:
        return limite

    nominal = nominal_dt.strftime("%Y-%m-%d")
    if nominal_dt.date() == entrega.fecha_limite_teorica.date():
        return limite

    ajuste = getattr(entrega, "tipo_ajuste", None)
    if ajuste == "GRACIA":
        return f"Vence {nominal} · plazo hasta {limite}"
    if ajuste == "ANTICIPADO":
        return f"Vence {nominal} · entregar antes de {limite}"
    return f"Vence {nominal} · límite {limite}"
