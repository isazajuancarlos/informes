"""Todas las combinaciones de AÑO, MES, DÍA y HORA, y qué campos exige cada una.

# Las cuatro dimensiones

    AÑO   cada año · cada N años
    MES   todos · varios · uno · (no aplica)
    DÍA   número fijo · varios números · último del mes · n-ésimo hábil ·
          ordinal de día de semana · día(s) de la semana · (todos)
    HORA  fin del día · franja de varias al día

Cada periodicidad necesita unas y solo unas: una semanal pide el día de la
SEMANA, no un número de día ni una fecha; una anual pide el día Y el mes; una de
varias veces al día pide la franja horaria.

**Preguntar de más no es inocuo.** Un formulario que muestra «día de corte» junto
a «último día del mes» invita a rellenar los dos, y entonces el motor descarta
uno en silencio: la obligación queda creada y no hace lo que su autor cree. Eso
no se arregla avisando — se arregla no preguntando.

# Lo que estas pruebas impiden

Que la declaración de `periodicidad.py` se separe de lo que el motor hace. La
tabla `CAMPOS_QUE_USA` se MIDE aquí, cambiando cada campo y viendo si el
resultado cambia. Si alguien añade un campo al motor y no lo declara, o declara
uno que el motor no lee, la prueba se cae.

Una tabla escrita de memoria envejece y vuelve a preguntar de más.
"""
from __future__ import annotations

import os
from datetime import date, timedelta

import pytest

os.environ.setdefault("DATABASE_URL", "sqlite:///./test_period.db")

from app.services.periodicidad import (  # noqa: E402
    CAMPOS_QUE_USA, CATALOGO, GRUPOS, campos_a_preguntar, campos_ignorados,
)
from app.services.scheduler import GeneradorTareas as G  # noqa: E402
from app.services.validar_regla import validar  # noqa: E402

INICIO = {"fecha_inicio": "2026-01-01"}
TODOS = list(range(1, 13))


def fechas(cfg, a1=2026, a2=2031):
    salida, dia = [], date(a1, 1, 1)
    while dia <= date(a2, 12, 31):
        salida += [v.nominal for v in G.obtener_vencimientos_para_fecha(dia, cfg)]
        dia += timedelta(days=1)
    return salida


# --------------------------------------------------------------------------
# Todas las combinaciones se pueden expresar, y dan lo que dicen
# --------------------------------------------------------------------------

#: (descripción, regla, vencimientos esperados en 6 años). Los números se
#: derivan de la periodicidad, no del motor: 6 años × 12 meses = 72.
COMBINACIONES = [
    ("mensual · día fijo",
     {"modo": "ESPECIFICO", "meses": TODOS, "dia_corte": 15, **INICIO}, 72),
    ("mensual · dos días fijos",
     {"modo": "ESPECIFICO", "meses": TODOS, "dias_corte": [7, 20], **INICIO}, 144),
    ("mensual · último día",
     {"modo": "ESPECIFICO", "meses": TODOS, "es_ultimo_dia": True, **INICIO}, 72),
    ("mensual · 5º día hábil",
     {"modo": "ESPECIFICO", "meses": TODOS, "dia_habil": 5, **INICIO}, 72),
    ("mensual · último martes",
     {"modo": "ESPECIFICO", "meses": TODOS, "dia_semana_mes": 1,
      "semana_del_mes": -1, **INICIO}, 72),
    ("trimestral · día fijo",
     {"modo": "ESPECIFICO", "meses": [3, 6, 9, 12], "dia_corte": 15, **INICIO}, 24),
    ("semestral · último día",
     {"modo": "ESPECIFICO", "meses": [6, 12], "es_ultimo_dia": True, **INICIO}, 12),
    ("anual · mes y día",
     {"modo": "ESPECIFICO", "meses": [3], "dia_corte": 15, **INICIO}, 6),
    ("bienal · mes y día",
     {"modo": "ESPECIFICO", "meses": [3], "dia_corte": 15,
      "cada_n_anios": 2, **INICIO}, 3),
    ("quinquenal · mes y día",
     {"modo": "ESPECIFICO", "meses": [3], "dia_corte": 15,
      "cada_n_anios": 5, **INICIO}, 2),
    ("semanal · un día de la semana",
     {"modo": "SEMANAL", "dias_semana": [0], **INICIO}, 313),
    ("semanal · dos días de la semana",
     {"modo": "SEMANAL", "dias_semana": [0, 3], **INICIO}, 626),
    ("cada N días",
     {"modo": "CICLICO", "intervalo": 30, **INICIO}, 74),
    ("una sola vez",
     {"modo": "CICLICO", "intervalo": 0, **INICIO}, 1),
    ("sin patrón · fechas sueltas",
     {"modo": "FECHAS", "fechas": ["2026-03-15", "2028-07-04"], **INICIO}, 2),
]


@pytest.mark.parametrize("nombre,regla,esperados", COMBINACIONES)
def test_cada_combinacion_genera_LO_QUE_DICE(nombre, regla, esperados):
    assert len(fechas(regla)) == esperados, nombre


@pytest.mark.parametrize("nombre,regla,_", COMBINACIONES)
def test_cada_combinacion_SE_PUEDE_GUARDAR(nombre, regla, _):
    """De nada sirve que el motor la calcule si el formulario la rechaza."""
    r = validar(regla)
    assert r.se_puede_guardar, f"{nombre}: {r.errores}"


def test_los_dias_de_la_semana_se_cuentan_SIN_el_motor():
    """Si el número esperado saliera del propio motor, la prueba sería circular.

    313 lunes en 2026-2031 se cuentan aquí con el calendario de Python.
    """
    lunes = sum(1 for k in range((date(2031, 12, 31) - date(2026, 1, 1)).days + 1)
                if (date(2026, 1, 1) + timedelta(days=k)).weekday() == 0)
    assert lunes == 313


# --------------------------------------------------------------------------
# La declaración no puede separarse del motor
# --------------------------------------------------------------------------

#: Un valor plausible por campo, para el sondeo. Los que van en PAREJA se
#: prueban juntos: sueltos no cambian nada y el sondeo los daría por ignorados.
SONDEO = {
    "intervalo": {"intervalo": 10},
    "cada_n_anios": {"cada_n_anios": 3},
    "meses": {"meses": [3]},
    "dia": {"dia_corte": 9},
    "dias_semana": {"dias_semana": [2]},
    "fechas": {"fechas": ["2026-05-05"]},
    "franja_horaria": {"hora_inicio": 9, "hora_fin": 17, "intervalo_horas": 4},
    "ajuste": {"ajuste_habil": True},
}

#: Bases mínimas por modo. La de FECHAS cae en FESTIVO a propósito: con una
#: fecha hábil el ajuste no tendría nada que mover y el sondeo lo daría por
#: ignorado — un falso negativo que ya se coló una vez.
BASES = {
    "CICLICO": {"modo": "CICLICO", "intervalo": 30, **INICIO},
    "ESPECIFICO": {"modo": "ESPECIFICO", "meses": [1, 2, 3], "dia_corte": 15, **INICIO},
    "SEMANAL": {"modo": "SEMANAL", "dias_semana": [0], **INICIO},
    "HOURLY": {"modo": "HOURLY", "hora_inicio": 8, "hora_fin": 18,
               "intervalo_horas": 2, **INICIO},
    "FECHAS": {"modo": "FECHAS", "fechas": ["2026-01-01"], **INICIO},
}


def huella(cfg):
    salida, dia = [], date(2026, 1, 1)
    for _ in range(400):
        salida += [(v.nominal, v.limite, v.hora)
                   for v in G.obtener_vencimientos_para_fecha(dia, cfg)]
        dia += timedelta(days=1)
    return salida


@pytest.mark.parametrize("modo", sorted(BASES))
def test_lo_declarado_coincide_con_lo_que_el_motor_USA_de_verdad(modo):
    """Se MIDE: se cambia cada grupo de campos y se mira si el resultado cambia.

    Si alguien añade un campo al motor y no lo declara, el asistente no lo
    preguntará nunca. Si declara uno que el motor no lee, lo preguntará para
    nada y el usuario creerá que hace algo.
    """
    base = BASES[modo]
    referencia = huella(base)

    medidos = set()
    for grupo, valores in SONDEO.items():
        if any(c in base for c in valores):
            medidos.add(grupo)   # ya está en la base: por definición lo usa
            continue
        if huella({**base, **valores}) != referencia:
            medidos.add(grupo)

    assert medidos == CAMPOS_QUE_USA[modo], (
        f"{modo}: el motor usa {sorted(medidos)} y está declarado "
        f"{sorted(CAMPOS_QUE_USA[modo])}")


# --------------------------------------------------------------------------
# Cada periodicidad pregunta lo suyo, y nada más
# --------------------------------------------------------------------------

@pytest.mark.parametrize("clave", sorted(CATALOGO))
def test_ninguna_periodicidad_pregunta_algo_que_su_modo_IGNORA(clave):
    """Preguntar de más invita a rellenar un campo que el motor descarta en
    silencio. Es el defecto que este módulo existe para hacer imposible."""
    p = CATALOGO[clave]
    sobra = set(p.pregunta) - CAMPOS_QUE_USA[p.modo]
    assert not sobra, f"{clave} pregunta {sorted(sobra)}, que {p.modo} ignora"


@pytest.mark.parametrize("clave", sorted(CATALOGO))
def test_ninguna_periodicidad_deja_SIN_preguntar_algo_que_hace_falta(clave):
    """El contrapunto: si no se pregunta, el usuario no puede contestarlo y la
    regla sale con el valor por defecto sin que nadie lo haya decidido."""
    p = CATALOGO[clave]
    faltan = CAMPOS_QUE_USA[p.modo] - set(p.pregunta)
    # Lo que la periodicidad ya decide por sí misma no se pregunta: «mensual»
    # son los doce meses y «diaria» es intervalo 1. Pedirlo sería hacer repetir
    # lo que se acaba de decir, y abrir la puerta a contradecirse.
    faltan -= set(p.fija)
    # Una periodicidad de un solo año no pregunta cada cuántos años.
    faltan -= {"cada_n_anios"}
    assert not faltan, f"{clave} no pregunta {sorted(faltan)}, y {p.modo} lo usa"


@pytest.mark.parametrize("clave", sorted(CATALOGO))
def test_lo_que_la_periodicidad_FIJA_produce_una_regla_valida(clave):
    """`regla_base()` es lo que el asistente pone antes de la primera pregunta.
    Si lo que fija fuera incoherente con su modo, el asistente arrancaría de una
    regla rota y nadie sabría por qué."""
    base = CATALOGO[clave].regla_base()
    assert base["modo"] == CATALOGO[clave].modo
    assert campos_ignorados(base) == [], f"{clave}: {campos_ignorados(base)}"


def test_una_SEMANAL_no_pregunta_por_numero_de_dia_ni_por_mes():
    """El ejemplo que da nombre a todo esto."""
    campos = campos_a_preguntar("SEMANAL")
    assert "dias_semana" in campos
    for prohibido in ("dia_corte", "dias_corte", "es_ultimo_dia", "meses",
                      "fechas", "hora_inicio"):
        assert prohibido not in campos, prohibido


def test_una_ANUAL_pregunta_el_mes_Y_el_dia():
    campos = campos_a_preguntar("ANUAL")
    assert "meses" in campos
    assert any(c in campos for c in ("dia_corte", "es_ultimo_dia", "dia_habil"))


def test_VARIAS_AL_DIA_pregunta_la_hora_y_NO_el_ajuste_a_habil():
    """Una franja horaria no se traslada a otro día: ofrecer el ajuste sería
    dar una opción que no hace nada."""
    campos = campos_a_preguntar("VARIAS_AL_DIA")
    assert "hora_inicio" in campos and "intervalo_horas" in campos
    assert "ajuste_habil" not in campos


# --------------------------------------------------------------------------
# Los campos que sobran se detectan
# --------------------------------------------------------------------------

def test_un_campo_puesto_que_el_modo_IGNORA_se_denuncia():
    """El dato está, quien lo puso cree que hace algo, y no hace nada. No se
    descubre nunca por su efecto, porque el efecto es que no pasa nada."""
    sobrantes = campos_ignorados(
        {"modo": "SEMANAL", "dias_semana": [0], "dia_corte": 15, "meses": [1]})
    assert set(sobrantes) == {"dia_corte", "meses"}, sobrantes

    r = validar({"modo": "SEMANAL", "dias_semana": [0], "dia_corte": 15, **INICIO})
    assert any("no se usarán" in a for a in r.advertencias), r.advertencias


def test_una_regla_limpia_no_denuncia_nada():
    """Un validador que se queja de todo entrena a ignorar los avisos."""
    assert campos_ignorados({"modo": "SEMANAL", "dias_semana": [0]}) == []
    assert campos_ignorados({"modo": "ESPECIFICO", "meses": TODOS,
                             "dia_corte": 15}) == []


def test_todos_los_grupos_declarados_pertenecen_a_algun_modo():
    """Un grupo que ningún modo use no se preguntaría nunca: es letra muerta."""
    usados = set()
    for grupos in CAMPOS_QUE_USA.values():
        usados |= grupos
    assert set(GRUPOS) == usados, set(GRUPOS) ^ usados
