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
    "cada_n_semanas": {"cada_n_semanas": 2},
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
    # Una periodicidad de un solo año no pregunta cada cuántos años, ni una
    # semanal cada cuántas semanas: su nombre ya lo dice.
    faltan -= {"cada_n_anios", "cada_n_semanas"}
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


# --------------------------------------------------------------------------
# Los patrones de una base real, EJECUTADOS
# --------------------------------------------------------------------------
#
# Antes se medía que eran «expresables» clasificando los códigos con expresiones
# regulares. Eso mide la FORMA del código, no lo que el motor hace con él. Aquí
# se traduce cada uno a una regla, se ejecuta, y se comprueba el resultado
# contra lo que el propio código declara —periodicidad y dirección del ajuste—,
# ninguna de las dos cosas sacada del motor.

def _patrones():
    import json
    import pathlib

    ruta = pathlib.Path(__file__).parent.parent / "herramientas" / "patrones_referencia.json"
    if not ruta.exists():
        pytest.skip("sin fichero de patrones de referencia")
    return json.load(open(ruta, encoding="utf-8"))


def test_los_patrones_reales_se_cumplen_TODOS():
    """Cero fallos. «Sin traducción» se cuenta aparte: es un límite de la
    notación de origen, no del sistema."""
    from herramientas.verificar_patrones import comprobar

    fallos = []
    for periodicidad, codigo in _patrones():
        estado, detalle = comprobar(periodicidad, codigo)
        if estado == "FALLA":
            fallos.append(f"{periodicidad} {codigo}: {detalle}")
    assert not fallos, fallos


def test_el_verificador_de_patrones_DISCRIMINA():
    """Una regla mal traducida a propósito tiene que salir FALLA.

    Sin esto, «79 de 80 cumplen» no informa de nada. La primera versión de la
    herramienta tenía un punto ciego justo aquí: las formas semanales pasaban
    sin comprobar nada, y una mutación del motor no la ponía roja.
    """
    from herramientas.verificar_patrones import comprobar

    from datetime import date

    from app.services.scheduler import (AJUSTE_ANTICIPADO, GeneradorTareas,
                                        Vencimiento)
    from herramientas import verificar_patrones as vp

    assert comprobar("MENSUAL", "M<15")[0] == "OK"
    assert comprobar("MENSUAL", "ESTO_NO_ES_UN_CODIGO")[0] == "SIN_TRADUCCION"

    # No se puede provocar el fallo desde la entrada: el traductor toma los
    # meses de la columna de periodicidad, así que la regla es coherente con lo
    # esperado por construcción. Lo que hay que romper es el MOTOR, que es
    # justo lo que la herramienta debe vigilar.
    original = GeneradorTareas.obtener_vencimientos_para_fecha
    try:
        GeneradorTareas.obtener_vencimientos_para_fecha = staticmethod(
            lambda fecha, config: [])
        estado, detalle = comprobar("MENSUAL", "M<15")
        assert estado == "FALLA" and "ningún vencimiento" in detalle, detalle

        # Y un ajuste que va en la dirección contraria a la que dice el código.
        def al_reves(fecha, config):
            if fecha != date(2026, 3, 15):
                return []
            return [Vencimiento(date(2026, 3, 15), date(2026, 3, 20),
                                AJUSTE_ANTICIPADO)]

        GeneradorTareas.obtener_vencimientos_para_fecha = staticmethod(al_reves)
        estado, detalle = comprobar("ANUAL", "A3<15")
        assert estado == "FALLA" and "hacia atrás" in detalle, detalle
    finally:
        GeneradorTareas.obtener_vencimientos_para_fecha = original

    # Y comprobado que vuelve a estar sano, o el resto del banco mentiría.
    assert comprobar("MENSUAL", "M<15")[0] == "OK"


# --------------------------------------------------------------------------
# El nombre de la periodicidad es una AFIRMACIÓN, y el motor sabe contar
# --------------------------------------------------------------------------

@pytest.mark.parametrize("clave,regla,ok", [
    ("ANUAL", {"modo": "ESPECIFICO", "meses": [3], "dia_corte": 15}, True),
    # Un informe anual entregado dos veces al año no es anual. Ni con dos días
    # en el mismo mes, ni con el mismo día en dos meses.
    ("ANUAL", {"modo": "ESPECIFICO", "meses": [3], "dias_corte": [5, 20]}, False),
    ("ANUAL", {"modo": "ESPECIFICO", "meses": [3, 9], "dia_corte": 5}, False),
    ("MENSUAL", {"modo": "ESPECIFICO", "meses": TODOS, "dia_corte": 15}, True),
    ("MENSUAL", {"modo": "ESPECIFICO", "meses": TODOS,
                 "dias_corte": [7, 10, 20]}, False),
    # En la quincenal la multiplicidad NO es una excepción: es la definición.
    ("QUINCENAL", {"modo": "ESPECIFICO", "meses": TODOS, "dias_corte": [15, 30],
                   "usar_ultimo_dia_en_meses_cortos": True}, True),
    ("TRIMESTRAL", {"modo": "ESPECIFICO", "meses": [3, 6, 9, 12],
                    "dia_corte": 15}, True),
    ("TRIMESTRAL", {"modo": "ESPECIFICO", "meses": TODOS, "dia_corte": 15}, False),
])
def test_la_regla_tiene_que_generar_LO_QUE_SU_NOMBRE_PROMETE(clave, regla, ok):
    r = validar({**regla, **INICIO}, periodicidad=clave)
    assert r.se_puede_guardar is ok, (clave, r.errores)


def test_cuando_el_nombre_no_cuadra_se_SUGIERE_el_correcto():
    """No se rechaza la FORMA —dos entregas al año es legítimo— sino el NOMBRE.
    Decir solo «no cuadra» deja al usuario adivinando."""
    r = validar({"modo": "ESPECIFICO", "meses": [3], "dias_corte": [5, 20],
                 **INICIO}, periodicidad="ANUAL")
    assert not r.se_puede_guardar
    assert "Semestral" in " ".join(r.errores), r.errores


def test_sin_declarar_periodicidad_no_se_exige_nada():
    """El formulario de una sola página no la pasa, y no puede empezar a
    rechazar reglas que antes guardaba."""
    r = validar({"modo": "ESPECIFICO", "meses": [3], "dias_corte": [5, 20], **INICIO})
    assert r.se_puede_guardar, r.errores


def test_una_periodicidad_de_numero_VARIABLE_no_se_comprueba():
    """Semanal puede dar 52 o 53, y con dos días marcados el doble. Exigirle un
    número fijo marcaría como error algo perfectamente correcto."""
    r = validar({"modo": "SEMANAL", "dias_semana": [0, 3], **INICIO},
                periodicidad="SEMANAL")
    assert r.se_puede_guardar, r.errores


def test_el_conteo_arranca_en_la_VIGENCIA_y_no_en_el_ano_natural():
    """Una regla que empieza en julio tendría la mitad de vencimientos en su
    primer año natural, y saldría marcada por una razón falsa."""
    r = validar({"modo": "ESPECIFICO", "meses": TODOS, "dia_corte": 15,
                 "fecha_inicio": "2026-07-01"}, periodicidad="MENSUAL")
    assert r.se_puede_guardar, r.errores


# --------------------------------------------------------------------------
# La jornada la declara la entidad; los festivos son otra cosa
# --------------------------------------------------------------------------

def dias_generados(regla, anio=2026):
    return len(fechas(regla, anio, anio))


def test_un_diario_puede_ser_de_domingo_a_domingo_o_de_lunes_a_viernes():
    """«Diario» no significa lo mismo en todas partes. En una entidad pública
    suele ser de lunes a viernes; en una privada puede incluir el sábado."""
    assert dias_generados({"modo": "CICLICO", "intervalo": 1, **INICIO}) == 365
    assert dias_generados({"modo": "SEMANAL", "dias_semana": [0, 1, 2, 3, 4],
                           **INICIO}) == 261
    assert dias_generados({"modo": "SEMANAL", "dias_semana": [0, 1, 2, 3, 4, 5],
                           **INICIO}) == 313


def test_omitir_festivos_NO_borra_los_sabados_de_una_jornada_que_los_trabaja():
    """El defecto que tuvo la primera versión: se comprobaba `_es_habil`, que
    lleva dentro la regla «lunes a viernes», así que una jornada de lunes a
    sábado perdía TODOS los sábados. El calendario imponiendo su jornada a la
    del cliente.

    Los días que la entidad trabaja los declara ella; lo que el calendario
    aporta es qué días son festivos.
    """
    lv = dias_generados({"modo": "SEMANAL", "dias_semana": [0, 1, 2, 3, 4],
                         "omitir_festivos": True, **INICIO})
    ls = dias_generados({"modo": "SEMANAL", "dias_semana": [0, 1, 2, 3, 4, 5],
                         "omitir_festivos": True, **INICIO})
    assert ls > lv, f"lunes a sábado ({ls}) debe superar a lunes a viernes ({lv})"
    assert ls - lv > 45, "se perdieron sábados"


def test_un_festivo_NO_genera_una_entrega_que_haya_que_poner_otro_dia():
    """Omitir no es ajustar. Tratar el festivo como los otros dos ajustes
    pondría el día siguiente DOS entregas —la del festivo y la suya propia— y
    eso es un incumplimiento inventado."""
    from app.services.scheduler import AJUSTE_OMITIR

    regla = {"modo": "SEMANAL", "dias_semana": [0, 1, 2, 3, 4],
             "omitir_festivos": True, **INICIO}
    venc = fechas(regla, 2026, 2026)
    assert date(2026, 1, 1) not in venc, "el 1 de enero es festivo"
    assert date(2026, 1, 2) in venc

    completos = [v for v in G.obtener_vencimientos_para_fecha(date(2026, 1, 2), regla)]
    assert completos[0].ajuste == AJUSTE_OMITIR
    assert not completos[0].se_movio, "omitir no mueve nada"


def test_sin_omitir_festivos_el_informe_se_entrega_igual():
    """Hay obligaciones diarias que no perdonan el festivo. Que se pueda omitir
    no significa que se omita siempre."""
    con = dias_generados({"modo": "SEMANAL", "dias_semana": [0, 1, 2, 3, 4],
                          "omitir_festivos": True, **INICIO})
    sin = dias_generados({"modo": "SEMANAL", "dias_semana": [0, 1, 2, 3, 4], **INICIO})
    assert sin > con and sin - con == 19, f"{sin} vs {con}"


def test_el_cuadro_de_posibilidades_NO_promete_nada_que_el_sistema_no_haga():
    """El cuadro se genera ejecutando el motor, no escribiéndolo.

    Un cuadro escrito a mano envejece y acaba prometiendo lo que el sistema ya
    no hace — y es lo que se le enseña a un cliente. Esta prueba lo regenera en
    cada corrida: si una fila deja de poder crearse, se cae.
    """
    from herramientas.cuadro_posibilidades import FILAS, al_anio

    rotas = []
    for dimension, posibilidad, regla, _ in FILAS:
        r = validar(regla)
        if not r.se_puede_guardar or al_anio(regla) == 0:
            rotas.append(f"{dimension}/{posibilidad}: {r.errores}")
    assert not rotas, rotas
    assert len(FILAS) >= 26, "el cuadro encogió"


# --------------------------------------------------------------------------
# «Cada N semanas» — y por qué «bisemanal» no es un nombre admisible
# --------------------------------------------------------------------------

def test_DOS_VECES_POR_SEMANA_no_es_una_periodicidad_sino_semanal_con_dos_dias():
    """Tuvo entrada propia en el catálogo y estaba mal.

    «Dos veces por semana» no es una periodicidad distinta: es SEMANAL con dos
    días marcados. Darle nombre propio creaba una segunda manera de decir lo
    mismo — el defecto que este módulo existe para impedir, cometido dentro del
    propio módulo.
    """
    claves = set(CATALOGO)
    assert "DOS_VECES_POR_SEMANA" not in claves, claves

    uno = fechas({"modo": "SEMANAL", "dias_semana": [1], **INICIO}, 2026, 2026)
    dos = fechas({"modo": "SEMANAL", "dias_semana": [2, 4], **INICIO}, 2026, 2026)
    assert len(uno) == 52 and len(dos) == 104
    # La misma periodicidad; lo que cambia es la respuesta a «qué días».
    assert CATALOGO["SEMANAL"].multiples_dias


def test_bisemanal_es_AMBIGUO_y_lleva_a_una_eleccion_en_vez_de_a_nada():
    """En español significa tanto «dos veces por semana» (104 al año) como
    «cada dos semanas» (26). La RAE admite los dos sentidos.

    La primera solución fue quitar la palabra del catálogo. Estaba mal: quitarla
    no la desambigua, la ESCONDE — si la norma del cliente dice «bisemanal» y no
    la encuentra, adivina. Ahora el término lleva a las dos y obliga a elegir,
    diciendo en qué se diferencian.
    """
    from app.services.periodicidad import desambiguar, es_ambiguo

    assert es_ambiguo("bisemanal")
    opciones = desambiguar("bisemanal")
    claves = {c for c, _, _ in opciones}
    assert claves == {"SEMANAL", "CADA_N_SEMANAS"}, claves
    # Y se dice en qué se distinguen: dos nombres sin diferencia visible dejan
    # al usuario eligiendo a cara o cruz.
    texto = " ".join(t for _, _, t in opciones)
    assert "104" in texto and "26" in texto, texto


def test_bianual_tambien_es_ambiguo():
    """«Cada dos años» o «dos veces al año». Ocurre de verdad en las normas."""
    from app.services.periodicidad import es_ambiguo

    assert es_ambiguo("bianual")


@pytest.mark.parametrize("termino", ["mensual", "trimestral", "anual",
                                     "cada dos semanas", "diario"])
def test_los_terminos_CLAROS_no_piden_desambiguar(termino):
    """Un sistema que pregunta por todo entrena a contestar sin leer."""
    from app.services.periodicidad import buscar, es_ambiguo

    assert buscar(termino), termino
    assert not es_ambiguo(termino), termino


def test_toda_periodicidad_se_puede_encontrar_por_su_nombre():
    """Una que no responda a ninguna búsqueda es inalcanzable en el asistente."""
    from app.services.periodicidad import buscar

    for clave, p in CATALOGO.items():
        assert clave in buscar(p.nombre), f"{clave} no se encuentra por su nombre"


@pytest.mark.parametrize("cada,esperado", [(1, 52), (2, 26), (3, 18), (4, 13)])
def test_cada_N_semanas_da_lo_que_dice(cada, esperado):
    venc = fechas({"modo": "SEMANAL", "dias_semana": [1],
                   "cada_n_semanas": cada, **INICIO}, 2026, 2026)
    assert len(venc) == esperado
    assert all(v.weekday() == 1 for v in venc), "todos deben ser martes"


def test_el_dia_de_la_semana_se_ELIGE_y_no_lo_impone_la_fecha_de_inicio():
    """Con «cada 14 días» el día lo decidía la vigencia: quien quisiera los
    martes tenía que saber poner un martes de fecha de inicio. Eso es pedirle al
    usuario que conozca la implementación."""
    desde_jueves = fechas({"modo": "SEMANAL", "dias_semana": [1],
                           "cada_n_semanas": 2, "fecha_inicio": "2026-01-01"},
                          2026, 2026)
    desde_martes = fechas({"modo": "SEMANAL", "dias_semana": [1],
                           "cada_n_semanas": 2, "fecha_inicio": "2026-01-06"},
                          2026, 2026)
    assert desde_jueves == desde_martes, "la vigencia no debe mover el patrón"
    assert desde_jueves[0] == date(2026, 1, 6), desde_jueves[0]


def test_se_ancla_en_la_PRIMERA_OCURRENCIA_y_no_en_la_semana_de_la_vigencia():
    """Anclar en la semana de la fecha de inicio se saltaba la primera entrega
    —el primer martes salía el 13 y no el 6— sin motivo que el usuario pudiera
    ver en su propia configuración."""
    venc = fechas({"modo": "SEMANAL", "dias_semana": [1], "cada_n_semanas": 2,
                   "fecha_inicio": "2026-01-01"}, 2026, 2026)
    assert venc[0] == date(2026, 1, 6)


@pytest.mark.parametrize("anio", [2024, 2027, 2030, 2033])
def test_cada_dos_semanas_no_se_desfasa_en_ningun_anio(anio):
    """El patrón tiene que seguir cayendo en el mismo día de la semana aunque
    el año tenga 53 semanas."""
    venc = fechas({"modo": "SEMANAL", "dias_semana": [1], "cada_n_semanas": 2,
                   "fecha_inicio": f"{anio}-01-01"}, anio, anio)
    assert all(v.weekday() == 1 for v in venc), anio
    for previo, siguiente in zip(venc, venc[1:]):
        assert (siguiente - previo).days == 14, f"{anio}: {previo} -> {siguiente}"


def test_el_ejemplo_completo_cada_dos_semanas_lunes_y_en_festivo_el_martes():
    """«Un informe cada dos semanas los lunes, y en festivo se entrega el
    martes.» Es el caso que reúne las tres dimensiones a la vez: cadencia de
    semanas, día de la semana elegido, y qué hacer si cae en festivo.

    Lo que se exige aquí es lo que hace útil la distinción: el vencimiento
    SIGUE siendo el lunes —el informe es el del lunes 20 de julio, no el del
    martes 21— y lo que se amplía es el plazo.
    """
    from app.services.preview_calendario import Vencimiento as Vista

    regla = {"modo": "SEMANAL", "dias_semana": [0], "cada_n_semanas": 2,
             "mover_habil": True, **INICIO}
    assert validar(regla).se_puede_guardar

    venc, _ = [], None
    dia = date(2026, 1, 1)
    while dia <= date(2026, 12, 31):
        venc += G.obtener_vencimientos_para_fecha(dia, regla)
        dia += timedelta(days=1)

    assert len(venc) == 26, len(venc)
    assert all(v.nominal.weekday() == 0 for v in venc), "todos vencen en LUNES"

    movidos = [v for v in venc if v.se_movio]
    assert movidos, "en 2026 hay lunes festivos en la serie"
    for v in movidos:
        assert v.limite.weekday() == 1, "el festivo se entrega el MARTES"
        assert v.es_gracia, "es plazo, no anticipación"
        texto = Vista(nominal=v.nominal, limite=v.limite, ajuste=v.ajuste).describir()
        assert "Vence lunes" in texto and "plazo hasta martes" in texto, texto

    # Y la misma regla con anticipación entrega el VIERNES anterior, no el
    # martes: la dirección la elige quien crea la obligación, no el sistema.
    anticipa = {**regla}
    anticipa.pop("mover_habil")
    anticipa["mover_habil_anterior"] = True
    dia, otros = date(2026, 1, 1), []
    while dia <= date(2026, 12, 31):
        otros += G.obtener_vencimientos_para_fecha(dia, anticipa)
        dia += timedelta(days=1)
    for v in (x for x in otros if x.se_movio):
        assert v.limite.weekday() == 4, "anticipar desde lunes cae en VIERNES"
