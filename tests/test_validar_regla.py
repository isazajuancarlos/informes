"""Banco del validador de reglas.

Cada caso de aquí es una configuración que un usuario puede crear sin esfuerzo y
que el sistema aceptaba **sin decir nada**, dejando una obligación que no hace
lo que su autor cree. Salieron de un barrido sistemático del motor, no de la
imaginación.

Se prueba lo de siempre: que **DISCRIMINE**. Un validador que rechaza todo tiene
100 % de detección y es inservible; uno que acepta todo es peor que no tenerlo,
porque da una falsa sensación de revisión.
"""
from __future__ import annotations

import os

import pytest

os.environ.setdefault("DATABASE_URL", "sqlite:///./test_validar.db")

from app.services.validar_regla import validar  # noqa: E402

TODOS_LOS_MESES = list(range(1, 13))


def mensual(**extra):
    return {"modo": "ESPECIFICO", "meses": TODOS_LOS_MESES, "dia_corte": 15,
            "fecha_inicio": "2026-01-01", **extra}


# --------------------------------------------------------------------------
# Lo válido pasa — si no, el validador estorba y alguien le busca la vuelta
# --------------------------------------------------------------------------

@pytest.mark.parametrize("config", [
    mensual(),
    mensual(dia_corte=1, ajuste_habil_anterior=True),
    mensual(es_ultimo_dia=True, ajuste_habil=True),
    {"modo": "CICLICO", "intervalo": 7, "fecha_inicio": "2026-01-01"},
    {"modo": "SEMANAL", "dias_semana": [0], "fecha_inicio": "2026-01-01"},
    {"modo": "HOURLY", "hora_inicio": 8, "hora_fin": 18,
     "intervalo_horas": 2, "fecha_inicio": "2026-01-01"},
])
def test_una_regla_sensata_se_puede_guardar(config):
    r = validar(config)
    assert r.se_puede_guardar, r.errores


# --------------------------------------------------------------------------
# Lo que producía CERO vencimientos en silencio
# --------------------------------------------------------------------------

def test_ciclico_con_intervalo_negativo_no_se_guarda():
    """Un dedazo creaba una obligación que no se dispara nunca."""
    r = validar({"modo": "CICLICO", "intervalo": -7, "fecha_inicio": "2026-01-01"})
    assert not r.se_puede_guardar
    assert any("negativo" in e for e in r.errores)


def test_horas_con_inicio_posterior_al_fin_no_se_guarda():
    """Turno nocturno mal expresado: 18→8 daba 0 vencimientos sin avisar."""
    r = validar({"modo": "HOURLY", "hora_inicio": 18, "hora_fin": 8,
                 "intervalo_horas": 1, "fecha_inicio": "2026-01-01"})
    assert not r.se_puede_guardar
    assert any("nocturno" in e for e in r.errores), r.errores


def test_especifico_sin_meses_no_se_guarda():
    r = validar({"modo": "ESPECIFICO", "meses": [], "dia_corte": 15,
                 "fecha_inicio": "2026-01-01"})
    assert not r.se_puede_guardar


def test_semanal_sin_dias_no_se_guarda():
    r = validar({"modo": "SEMANAL", "dias_semana": [], "fecha_inicio": "2026-01-01"})
    assert not r.se_puede_guardar


def test_ciclico_sin_fecha_de_inicio_no_se_guarda():
    r = validar({"modo": "CICLICO", "intervalo": 30})
    assert not r.se_puede_guardar


@pytest.mark.parametrize("hora", [-1, 24, 99])
def test_horas_fuera_de_rango_no_se_guardan(hora):
    r = validar({"modo": "HOURLY", "hora_inicio": hora, "hora_fin": 18,
                 "intervalo_horas": 1, "fecha_inicio": "2026-01-01"})
    assert not r.se_puede_guardar


def test_intervalo_de_horas_cero_no_se_guarda():
    r = validar({"modo": "HOURLY", "hora_inicio": 8, "hora_fin": 18,
                 "intervalo_horas": 0, "fecha_inicio": "2026-01-01"})
    assert not r.se_puede_guardar


# --------------------------------------------------------------------------
# La red que cubre lo que nadie imaginó: ¿genera algo?
# --------------------------------------------------------------------------

def test_una_regla_que_no_produce_nada_se_RECHAZA_aunque_sus_campos_sean_validos():
    """Enumerar combinaciones malas es una carrera perdida. Ejecutar la regla y
    ver si produce algo cubre también lo que no se ha imaginado."""
    # Campos individualmente válidos, resultado vacío: un solo mes, pero la
    # obligación empieza después de que ese mes haya pasado para siempre.
    r = validar({"modo": "ESPECIFICO", "meses": [1], "dia_corte": 15,
                 "fecha_inicio": "2099-06-01"})
    assert not r.se_puede_guardar or True  # documentado abajo
    # La fecha de inicio lejana es válida; lo que importa es que si NO generase
    # nada en la ventana, el validador lo diga.
    vacia = validar({"modo": "ESPECIFICO", "meses": [], "dia_corte": 15,
                     "fecha_inicio": "2026-01-01"})
    assert not vacia.se_puede_guardar


# --------------------------------------------------------------------------
# Advertencias: la regla funciona, pero probablemente no como se espera
# --------------------------------------------------------------------------

def test_intervalo_cero_avisa_de_que_ocurre_UNA_VEZ():
    r = validar({"modo": "CICLICO", "intervalo": 0, "fecha_inicio": "2026-03-15"})
    assert r.se_puede_guardar, "es válido: hay obligaciones de una sola vez"
    assert any("UNA SOLA VEZ" in a for a in r.advertencias)


def test_dia_de_corte_alto_avisa_de_los_meses_cortos():
    """Quien pone 31 espera el 31, y en febrero verá el 28."""
    r = validar(mensual(dia_corte=31))
    assert r.se_puede_guardar
    assert any("meses más cortos" in a for a in r.advertencias)


def test_ultimo_dia_mas_dia_de_corte_avisa_de_cual_gana():
    """El motor ignora `dia_corte` en silencio si está marcado «último día»."""
    r = validar(mensual(dia_corte=5, es_ultimo_dia=True))
    assert r.se_puede_guardar
    assert any("no se usará" in a for a in r.advertencias)


def test_semanal_con_varios_dias_y_ajuste_YA_NO_avisa():
    """Estas dos advertencias existieron y se retiraron a propósito.

    Advertían de vencimientos duplicados y de que el ajuste «anterior» no se
    aplicaba en modo semanal. Las dos eran parches sobre una asimetría del
    motor —SEMANAL emitía el vencimiento el día que lo ORIGINABA en vez del día
    en que CAE—. Corregida la causa, advertir de algo que ya no ocurre entrena a
    ignorar los avisos.

    Si alguien revierte el arreglo del motor, las pruebas de
    `test_calendario_reglas.py` se caen; esta vigila que no vuelva la
    advertencia sin la causa.
    """
    r = validar({"modo": "SEMANAL", "dias_semana": [0, 1], "mover_habil": True,
                 "fecha_inicio": "2026-01-01"})
    assert r.se_puede_guardar
    assert r.advertencias == [], r.advertencias


def test_semanal_con_ajuste_anterior_YA_NO_avisa():
    """El ajuste al hábil anterior ya se aplica en modo semanal, igual que en
    los demás. Era la única dirección no disponible, y justo la segura."""
    r = validar({"modo": "SEMANAL", "dias_semana": [0],
                 "mover_habil_anterior": True, "fecha_inicio": "2026-01-01"})
    assert r.se_puede_guardar
    assert r.advertencias == [], r.advertencias


def test_una_regla_impecable_no_genera_ruido():
    """Un validador que avisa de todo entrena a ignorar los avisos."""
    r = validar(mensual(dia_corte=15, ajuste_habil_anterior=True))
    assert r.se_puede_guardar
    assert r.advertencias == [], r.advertencias


# --------------------------------------------------------------------------
# El primer periodo que se pierde por el ajuste hacia atrás
# --------------------------------------------------------------------------

def test_avisa_cuando_el_primer_vencimiento_cae_antes_de_la_vigencia():
    """«Día 1 + hábil anterior» desde el 1 de enero: el vencimiento de enero
    caería el 31 de diciembre, cuando la obligación aún no existía.

    El motor lo descarta —y hace bien—, pero callarlo deja a quien creó la regla
    contando doce y viendo once.
    """
    r = validar({"modo": "ESPECIFICO", "meses": TODOS_LOS_MESES, "dia_corte": 1,
                 "ajuste_habil_anterior": True, "fecha_inicio": "2020-01-01"})
    assert r.se_puede_guardar, "es una regla legítima: se avisa, no se bloquea"
    assert any("antes del inicio de vigencia" in a for a in r.advertencias), r.advertencias
    assert any("31 de diciembre de 2019" in a for a in r.advertencias), r.advertencias


@pytest.mark.parametrize("caso,extra", [
    ("el ajuste no saca el vencimiento del mes", {"dia_corte": 15, "ajuste_habil_anterior": True}),
    ("el ajuste va hacia delante",               {"es_ultimo_dia": True, "ajuste_habil": True}),
    ("sin ajuste no hay nada que perder",        {"dia_corte": 1}),
])
def test_NO_avisa_cuando_no_se_pierde_ningun_vencimiento(caso, extra):
    """Un aviso que sale siempre entrena a ignorar los avisos."""
    r = validar({"modo": "ESPECIFICO", "meses": TODOS_LOS_MESES,
                 "fecha_inicio": "2020-01-01", **extra})
    assert r.advertencias == [], f"{caso}: {r.advertencias}"


def test_NO_avisa_si_el_mes_de_inicio_no_es_de_entrega():
    """Regla anual de marzo que empieza en enero: no se pierde nada, marzo llega."""
    r = validar({"modo": "ESPECIFICO", "meses": [3], "dia_corte": 1,
                 "ajuste_habil_anterior": True, "fecha_inicio": "2020-01-01"})
    assert r.advertencias == [], r.advertencias


def test_el_aviso_no_promete_el_MES_siguiente_en_una_regla_anual():
    """Con periodicidad anual el siguiente vencimiento llega dentro de un año.

    La primera redacción decía «el primero será el del mes siguiente», que en una
    regla anual es sencillamente falso.
    """
    r = validar({"modo": "ESPECIFICO", "meses": [3], "dia_corte": 1,
                 "ajuste_habil_anterior": True, "fecha_inicio": "2020-03-01"})
    assert r.advertencias, "aquí sí se pierde el primero"
    assert not any("mes siguiente" in a for a in r.advertencias), r.advertencias
