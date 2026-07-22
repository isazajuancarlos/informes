"""La traducción entre el FORMULARIO y la REGLA, que no tenía pruebas.

Entre «el motor calcula bien» y «el producto hace lo correcto» hay un paso que
nadie comprobaba: lo que el formulario de creación arma a partir de lo que el
usuario marca en pantalla. Si esa traducción pierde una bandera, el motor
funciona impecablemente sobre una regla que no es la que se pidió — y la vista
previa muestra lo mismo que se guardará, así que ni siquiera se nota.

Se prueba en las dos direcciones:

  - **Ida**: la opción del selector produce las banderas correctas.
  - **Vuelta**: al reabrir un informe guardado, el selector muestra lo que el
    sistema está haciendo de verdad, no lo que la interfaz preferiría.

La ida y la vuelta tienen que componer: si no, editar un informe sin tocar la
periodicidad la cambiaría sola. Es el defecto más caro de este bloque porque no
deja rastro: nadie edita un informe esperando que cambien sus vencimientos.
"""
from __future__ import annotations

import os
from datetime import date

import pytest

os.environ.setdefault("DATABASE_URL", "sqlite:///./test_form.db")
os.environ.setdefault("DEPLOYMENT_PROFILE", "standalone")

from app.services.preview_calendario import proximos  # noqa: E402
from app.ui.reports_manager import (  # noqa: E402
    AJUSTE_HABIL,
    _ajuste_desde_banderas,
    _banderas_desde_ajuste,
    _construir_regla,
)

OPCIONES = list(AJUSTE_HABIL.keys())


def formulario(modo, **bloques):
    """El estado del formulario, con los mismos valores por defecto que la UI."""
    datos = {
        'modo_tiempo': modo,
        'fecha_inicio': '2026-01-01',
        'tiempo_ciclico': {'anos': 0, 'meses': 0, 'dias': 0, 'total_dias': 0,
                           'ajuste': 'ninguno'},
        'tiempo_especifico': {'meses_seleccionados': [], 'dia_corte': 1,
                              'es_ultimo_dia': False, 'ajuste': 'ninguno'},
        'tiempo_semanal': {'dias_seleccionados': [], 'ajuste': 'ninguno'},
        'tiempo_horas': {'hora_inicio': 8, 'hora_fin': 18, 'intervalo': 1},
    }
    for clave, valores in bloques.items():
        datos[clave].update(valores)
    return datos


# --------------------------------------------------------------------------
# Las tres opciones existen y significan cosas distintas
# --------------------------------------------------------------------------

def test_el_selector_ofrece_exactamente_tres_opciones():
    assert set(OPCIONES) == {"ninguno", "anterior", "siguiente"}


def test_los_textos_NO_prometen_que_el_vencimiento_se_mueve():
    """El día de vencimiento es el que fija la norma. Redactar las opciones como
    «mover la fecha» era lo que hacía creer que la obligación cambiaba de sitio,
    y con ella el periodo al que pertenece."""
    for clave, texto in AJUSTE_HABIL.items():
        assert "Mover" not in texto, f"{clave}: «{texto}»"

    assert "no cambia" in AJUSTE_HABIL["anterior"], AJUSTE_HABIL["anterior"]
    assert "gracia" in AJUSTE_HABIL["siguiente"], AJUSTE_HABIL["siguiente"]


# --------------------------------------------------------------------------
# Ida y vuelta: editar sin tocar nada no puede cambiar la periodicidad
# --------------------------------------------------------------------------

@pytest.mark.parametrize("opcion", OPCIONES)
@pytest.mark.parametrize("siguiente,anterior", [
    ("ajuste_habil", "ajuste_habil_anterior"),
    ("mover_habil", "mover_habil_anterior"),
])
def test_la_ida_y_la_vuelta_componen(opcion, siguiente, anterior):
    """opción -> banderas -> opción tiene que devolver la misma opción.

    Las dos parejas de nombres se prueban porque el modo ESPECÍFICO usa una y
    el CÍCLICO y el SEMANAL la otra. Esa divergencia ya costó que el ajuste al
    hábil anterior no se aplicara nunca en modo semanal.
    """
    banderas = _banderas_desde_ajuste(opcion, siguiente, anterior)
    vuelta = _ajuste_desde_banderas(banderas.get(siguiente), banderas.get(anterior))
    assert vuelta == opcion, f"{opcion} -> {banderas} -> {vuelta}"


def test_las_dos_banderas_a_la_vez_muestran_lo_que_el_motor_HACE():
    """Un informe viejo puede traer las dos. El motor resuelve por su `elif`:
    gana «siguiente». La interfaz tiene que decir eso, no lo que preferiría."""
    assert _ajuste_desde_banderas(True, True) == "siguiente"


# --------------------------------------------------------------------------
# El formulario produce una regla que el motor entiende
# --------------------------------------------------------------------------

@pytest.mark.parametrize("opcion", OPCIONES)
def test_mensual_del_dia_1_llega_al_motor_con_su_ajuste(opcion):
    """El caso que dio nombre a todo esto: informe con fecha fija el día 1.

    Se comprueba de punta a punta —formulario, regla, motor— y con el resultado
    esperado por opción, no solo que «no reviente».
    """
    datos = formulario('ESPECIFICO', tiempo_especifico={
        'meses_seleccionados': [1], 'dia_corte': 1, 'ajuste': opcion})
    regla = _construir_regla(datos)

    venc, aviso = proximos(regla, cuantos=1, desde=date(2026, 1, 1))
    assert venc, f"{opcion}: no generó nada ({aviso})"
    v = venc[0]

    # El día NOMINAL es el 1 de enero en las tres opciones. Eso es lo que
    # significa que el vencimiento no se mueve.
    assert v.nominal == date(2026, 1, 1), f"{opcion}: nominal {v.nominal}"

    if opcion == "ninguno":
        assert v.limite == v.nominal
    elif opcion == "anterior":
        assert v.limite < v.nominal, f"anticipada va hacia atrás: {v.limite}"
    else:
        assert v.limite > v.nominal, f"la gracia va hacia delante: {v.limite}"


@pytest.mark.parametrize("modo,bloque,extra", [
    ('CICLICO', 'tiempo_ciclico', {'total_dias': 30}),
    ('ESPECIFICO', 'tiempo_especifico', {'meses_seleccionados': [3, 6, 9, 12],
                                         'dia_corte': 15}),
    ('SEMANAL', 'tiempo_semanal', {'dias_seleccionados': [0, 3]}),
])
@pytest.mark.parametrize("opcion", ["anterior", "siguiente"])
def test_el_ajuste_llega_en_TODOS_los_modos(modo, bloque, extra, opcion):
    """El ajuste elegido tiene que cambiar algo en los tres modos.

    Si en alguno se perdiera por el camino, el usuario elegiría una cosa y
    obtendría otra sin ningún aviso — que es exactamente lo que pasaba en modo
    semanal con la opción «anterior».
    """
    con = _construir_regla(formulario(modo, **{bloque: {**extra, 'ajuste': opcion}}))
    sin = _construir_regla(formulario(modo, **{bloque: {**extra, 'ajuste': 'ninguno'}}))
    assert con != sin, f"{modo}/{opcion}: el ajuste no llegó a la regla"

    v_con, _ = proximos(con, cuantos=12, desde=date(2026, 1, 1))
    v_sin, _ = proximos(sin, cuantos=12, desde=date(2026, 1, 1))
    assert v_con and v_sin

    # Los días nominales son los mismos: el ajuste no mueve la obligación.
    assert [v.nominal for v in v_con] == [v.nominal for v in v_sin], modo
    # Y alguno tiene que haberse ajustado, o no se estaría probando nada.
    assert any(v.se_ajusto for v in v_con), f"{modo}/{opcion}: no se ajustó ninguno"


def test_horas_no_pierde_la_franja():
    regla = _construir_regla(formulario('HOURLY', tiempo_horas={
        'hora_inicio': 6, 'hora_fin': 22, 'intervalo': 4}))
    assert regla['hora_inicio'] == 6 and regla['hora_fin'] == 22
    assert regla['intervalo_horas'] == 4


def test_la_fecha_de_inicio_llega_en_todos_los_modos():
    """Sin ella el motor no genera nada, y el validador lo rechaza. Perderla en
    la traducción daría un «esta regla no genera ningún vencimiento» que no se
    explica mirando el formulario."""
    for modo in ('CICLICO', 'ESPECIFICO', 'SEMANAL', 'HOURLY'):
        regla = _construir_regla(formulario(modo))
        assert regla.get('fecha_inicio') == '2026-01-01', modo
