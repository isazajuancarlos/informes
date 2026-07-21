"""Banco de la conversión UTC → hora local.

Existe porque el registro de auditoría de seguridad guardaba en UTC y se
mostraba como si fuera local: en Colombia, cada evento aparecía **cinco horas en
el futuro**. Un rastro de auditoría con las horas corridas no está incompleto:
miente, y en silencio.

Lo que se prueba no es que la función devuelva algo, sino que **la diferencia
sea la correcta** — una conversión que no convierte pasaría cualquier prueba que
solo mirase el tipo de dato.
"""
from __future__ import annotations

from datetime import datetime, timedelta, timezone

import pytest

from app.core import tiempo


def test_convierte_utc_a_bogota_restando_cinco_horas(monkeypatch):
    """El caso concreto que estaba roto. Colombia es UTC-5 todo el año: no tiene
    horario de verano, así que el desfase no depende de la fecha."""
    monkeypatch.setattr(tiempo, "TIMEZONE", "America/Bogota")
    utc = datetime(2026, 7, 21, 20, 17, 42)  # naive, como lo guarda la BD
    local = tiempo.utc_a_local(utc)
    assert (local.hour, local.minute) == (15, 17)
    assert local.utcoffset() == timedelta(hours=-5)


def test_el_formato_es_el_que_espera_la_tabla(monkeypatch):
    monkeypatch.setattr(tiempo, "TIMEZONE", "America/Bogota")
    assert tiempo.formatear_utc(datetime(2026, 7, 21, 20, 17, 42)) == "2026-07-21 15:17:42"


def test_none_no_revienta_y_da_cadena_vacia():
    """La columna es nullable y la tabla lo pintaba con un `if`; no se puede
    perder ese comportamiento al cambiar la función."""
    assert tiempo.utc_a_local(None) is None
    assert tiempo.formatear_utc(None) == ""


def test_respeta_un_momento_que_YA_trae_zona(monkeypatch):
    """Por si algún día se migra la columna a datetimes con zona: entonces la
    conversión debe seguir siendo correcta y no volver a restar cinco horas."""
    monkeypatch.setattr(tiempo, "TIMEZONE", "America/Bogota")
    con_zona = datetime(2026, 7, 21, 20, 17, 42, tzinfo=timezone.utc)
    assert tiempo.formatear_utc(con_zona) == "2026-07-21 15:17:42"


def test_cambiar_de_zona_cambia_el_resultado(monkeypatch):
    """Si la zona configurada NO afectara al resultado, la función sería
    decorativa y nadie lo notaría."""
    utc = datetime(2026, 7, 21, 20, 0, 0)
    monkeypatch.setattr(tiempo, "TIMEZONE", "America/Bogota")
    bogota = tiempo.formatear_utc(utc)
    monkeypatch.setattr(tiempo, "TIMEZONE", "Europe/Madrid")
    madrid = tiempo.formatear_utc(utc)
    assert bogota != madrid


def test_una_zona_inexistente_cae_a_UTC_y_no_a_la_del_servidor(monkeypatch):
    """Fallar a UTC es visible y alguien lo corrige. Fallar a la hora local del
    servidor parece correcto y no se entera nadie nunca (directiva 20)."""
    monkeypatch.setattr(tiempo, "TIMEZONE", "Marte/Olympus_Mons")
    utc = datetime(2026, 7, 21, 20, 17, 42)
    assert tiempo.formatear_utc(utc) == "2026-07-21 20:17:42"


def test_el_desfase_que_habia_era_de_cinco_horas(monkeypatch):
    """Prueba de regresión del defecto concreto: mostrar el UTC crudo y mostrar
    el convertido tienen que diferir exactamente en el offset de la zona."""
    monkeypatch.setattr(tiempo, "TIMEZONE", "America/Bogota")
    utc = datetime(2026, 7, 21, 20, 17, 42)
    crudo = utc.strftime("%Y-%m-%d %H:%M:%S")          # lo que hacía audit.py
    corregido = tiempo.formatear_utc(utc)               # lo que hace ahora
    assert crudo != corregido
    diferencia = utc - tiempo.utc_a_local(utc).replace(tzinfo=None)
    assert diferencia == timedelta(hours=5)
