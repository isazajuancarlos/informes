"""Banco del cálculo de vencimientos — el paso más importante del sistema.

De la definición de la obligación sale todo lo demás: el calendario, las tareas,
las alertas y la trazabilidad. Una regla que se entiende mal no falla
ruidosamente: **genera vencimientos distintos de los que la persona creía, o
ninguno**, y eso no se descubre hasta que lo pregunta un ente de control.

El defecto que motivó este banco (2026-07-21): en modo ESPECÍFICO el motor solo
miraba el mes de la fecha evaluada, pero el ajuste a día hábil puede sacar el
objetivo de ese mes —«día 1 + hábil anterior» manda el vencimiento de febrero a
enero—. Resultado medido sobre 2026:

    día 1 con ajuste ANTERIOR ......  6 de 12 vencimientos
    último día con ajuste SIGUIENTE   8 de 12

Seis y cuatro obligaciones al año que **nunca aparecían**: sin tarea, sin alerta
y sin rastro.
"""
from __future__ import annotations

import os
from datetime import date, timedelta

import pytest

os.environ.setdefault("DATABASE_URL", "sqlite:///./test_calendario.db")

from app.services.scheduler import GeneradorTareas as G  # noqa: E402

TODOS_LOS_MESES = list(range(1, 13))


def vencimientos(config, desde, hasta):
    """Los vencimientos que el motor genera en el rango, por día NOMINAL."""
    salida, dia = [], desde
    while dia <= hasta:
        salida += G.obtener_vencimientos_para_fecha(dia, config)
        dia += timedelta(days=1)
    return salida


def barrer(config, desde, hasta):
    """Solo los días LÍMITE, para las pruebas que preguntan por ellos."""
    return [v.limite for v in vencimientos(config, desde, hasta)]


def mensual(**extra):
    return {"modo": "ESPECIFICO", "meses": TODOS_LOS_MESES,
            "fecha_inicio": "2026-01-01", **extra}


# --------------------------------------------------------------------------
# El defecto del cruce de mes — una prueba por combinación
# --------------------------------------------------------------------------

#: Configuraciones mensuales que un usuario puede crear. Se prueban TODAS contra
#: TODOS los años del rango, no contra un año de ejemplo.
CONFIGS_MENSUALES = [
    ("día 1, sin ajuste",            {"dia_corte": 1}),
    ("día 1, hábil siguiente",       {"dia_corte": 1, "ajuste_habil": True}),
    ("día 1, hábil ANTERIOR",        {"dia_corte": 1, "ajuste_habil_anterior": True}),
    ("día 15, hábil siguiente",      {"dia_corte": 15, "ajuste_habil": True}),
    ("día 15, hábil anterior",       {"dia_corte": 15, "ajuste_habil_anterior": True}),
    ("día 28, hábil anterior",       {"dia_corte": 28, "ajuste_habil_anterior": True}),
    ("día 29, hábil siguiente",      {"dia_corte": 29, "ajuste_habil": True}),
    ("día 31, hábil siguiente",      {"dia_corte": 31, "ajuste_habil": True}),
    ("día 31, hábil anterior",       {"dia_corte": 31, "ajuste_habil_anterior": True}),
    ("último día, sin ajuste",       {"es_ultimo_dia": True}),
    ("último día, hábil SIGUIENTE",  {"es_ultimo_dia": True, "ajuste_habil": True}),
    ("último día, hábil anterior",   {"es_ultimo_dia": True, "ajuste_habil_anterior": True}),
]

#: Rango de años. Cubre bisiestos, años que empiezan en cada día de la semana y
#: veinte configuraciones distintas de festivos móviles.
#:
#: **Un año concreto es un EJEMPLO, no la especificación.** La primera versión de
#: este banco solo probaba 2026 y daba todo por bueno; el barrido de 2020-2040
#: destapó que «último día + siguiente» generaba un vencimiento originado en el
#: mes ANTERIOR a la vigencia. Con un solo año eso no se ve.
ANIOS = range(2020, 2041)


def origen(v):
    """El día NOMINAL del vencimiento — el que fija la regla.

    Contar por año natural no funciona: con «plazo de gracia» sobre el último
    día del mes, la entrega de diciembre se admite hasta enero del año siguiente
    y el año parece tener once. El día nominal es la unidad estable, y es la que
    importa jurídicamente: la obligación pertenece al periodo que la origina, no
    al día en que se alcanza a entregar.
    """
    return v.nominal


@pytest.mark.parametrize("nombre,extra", CONFIGS_MENSUALES)
def test_una_obligacion_mensual_genera_UN_VENCIMIENTO_POR_MES_TODOS_LOS_ANIOS(nombre, extra):
    """Un mes, un vencimiento — en cualquier año y con cualquier ajuste.

    Se piden los doce primeros desde el 1 de enero y se comprueba que sus meses
    de ORIGEN son enero..diciembre, en orden y sin repetir. Así la prueba no
    depende de dónde caigan las fechas, que es justo lo que el ajuste altera.
    """
    from app.services.preview_calendario import proximos

    for anio in ANIOS:
        cfg = {"modo": "ESPECIFICO", "meses": TODOS_LOS_MESES,
               "fecha_inicio": f"{anio}-01-01", **extra}
        venc, aviso = proximos(cfg, cuantos=12, desde=date(anio, 1, 1))
        assert len(venc) == 12, f"{nombre} en {anio}: {len(venc)} de 12 ({aviso})"

        origenes = [(origen(v).year, origen(v).month) for v in venc]

        # Consecutivos, sin huecos ni repeticiones. Es la propiedad que de
        # verdad importa: un mes que se salta en medio es una obligación
        # incumplida que nadie ve.
        for previo, siguiente in zip(origenes, origenes[1:]):
            esperado = (previo[0] + 1, 1) if previo[1] == 12 else (previo[0], previo[1] + 1)
            assert siguiente == esperado, (
                f"{nombre} en {anio}: hueco o repetición entre {previo} y "
                f"{siguiente} -> {origenes}")

        # El primer origen es el mes en que arranca la vigencia, SALVO que su
        # vencimiento cayera antes de esa fecha —con «hábil anterior», el día 1
        # de enero se mueve al 31 de diciembre del año previo, cuando la
        # obligación aún no existía—. Ese salto es correcto y es el único que se
        # admite; comprobarlo contra la causa evita que tape un fallo real.
        primero = origenes[0]
        if primero != (anio, 1):
            assert primero == (anio, 2), f"{nombre} en {anio}: empieza en {primero}"
            objetivo = G._objetivo_del_mes(anio, 1, cfg)
            assert objetivo is not None and objetivo < date(anio, 1, 1), (
                f"{nombre} en {anio}: se saltó enero sin que su vencimiento "
                f"({objetivo}) cayera antes de la vigencia")

        fechas = [v.fecha for v in venc]
        assert len(set(fechas)) == 12, f"{nombre} en {anio}: fechas duplicadas {fechas}"
        assert fechas == sorted(fechas), f"{nombre} en {anio}: desordenadas"


@pytest.mark.parametrize("nombre,extra", CONFIGS_MENSUALES)
def test_el_ajuste_respeta_su_direccion_TODOS_LOS_ANIOS(nombre, extra):
    """«Anterior» nunca puede mover hacia adelante, ni «siguiente» hacia atrás.

    Es la propiedad de la que depende la seguridad jurídica del plazo: adelantar
    nunca llega tarde, aplazar sí.
    """
    from app.services.preview_calendario import proximos

    hacia_atras = extra.get("ajuste_habil_anterior")
    hacia_alante = extra.get("ajuste_habil")
    if not (hacia_atras or hacia_alante):
        pytest.skip("sin ajuste no hay dirección que comprobar")

    for anio in ANIOS:
        cfg = {"modo": "ESPECIFICO", "meses": TODOS_LOS_MESES,
               "fecha_inicio": f"{anio}-01-01", **extra}
        venc, _ = proximos(cfg, cuantos=12, desde=date(anio, 1, 1))
        for v in venc:
            if not v.se_ajusto:
                continue
            if hacia_atras:
                assert v.fecha < v.sin_ajustar, f"{nombre} en {anio}: {v.fecha} no está antes de {v.sin_ajustar}"
            else:
                assert v.fecha > v.sin_ajustar, f"{nombre} en {anio}: {v.fecha} no está después de {v.sin_ajustar}"


@pytest.mark.parametrize("nombre,extra", CONFIGS_MENSUALES)
def test_ningun_vencimiento_nace_antes_de_la_vigencia_TODOS_LOS_ANIOS(nombre, extra):
    """El MES que origina el vencimiento debe estar dentro de vigencia, no solo
    el día en que cae.

    Sin esto, una obligación que empieza el 1 de enero generaba un vencimiento
    el 2 de enero originado en el 31 de diciembre ANTERIOR. Se veía en 2024:
    trece vencimientos en vez de doce.
    """
    from app.services.preview_calendario import proximos

    for anio in ANIOS:
        cfg = {"modo": "ESPECIFICO", "meses": TODOS_LOS_MESES,
               "fecha_inicio": f"{anio}-03-01", **extra}
        venc, _ = proximos(cfg, cuantos=6, desde=date(anio, 1, 1))
        assert venc, f"{nombre} en {anio}: no generó nada"
        primero = origen(venc[0])
        # Nunca ANTES de marzo. Puede ser abril si el vencimiento de marzo caía
        # antes del 1 de marzo por el ajuste hacia atrás; lo prohibido es nacer
        # en un mes anterior a la vigencia.
        assert (primero.year, primero.month) >= (anio, 3), (
            f"{nombre} en {anio}: el primer vencimiento nace en "
            f"{primero}, antes del inicio de vigencia ({anio}-03-01)")
        assert (primero.year, primero.month) <= (anio, 4), (
            f"{nombre} en {anio}: se saltó más de un mes -> {primero}")


@pytest.mark.parametrize("extra", [
    {"dia_corte": 1, "ajuste_habil_anterior": True},
    {"es_ultimo_dia": True, "ajuste_habil": True},
])
def test_ningun_mes_se_queda_sin_vencimiento(extra):
    """Lo que de verdad importaba: que no falte NINGÚN mes.

    Contar doce no bastaría — podrían salir dos de un mes y ninguno de otro.
    """
    fechas = barrer(mensual(**extra), date(2026, 1, 1), date(2026, 12, 31))
    # El vencimiento de un mes puede caer en el mes vecino por el ajuste, así
    # que se cuentan los vencimientos, no los meses en que caen.
    assert len(fechas) == 12
    huecos = []
    for m in TODOS_LOS_MESES:
        cerca = [f for f in fechas if abs((f - date(2026, m, 15)).days) <= 20]
        if not cerca:
            huecos.append(m)
    assert not huecos, f"meses sin vencimiento cercano: {huecos}"


def test_el_ajuste_anterior_mueve_HACIA_ATRAS_y_no_hacia_delante():
    """La dirección del ajuste no es un detalle cosmético.

    Si un vencimiento legal cae en sábado, moverlo al lunes puede ser TARDE;
    moverlo al viernes es seguro. Quien pide «anterior» pide lo seguro, y
    devolverle lo contrario sería el peor error posible en un sistema de
    cumplimiento.
    """
    # 2026-08-01 es sábado.
    cfg = mensual(dia_corte=1, ajuste_habil_anterior=True)
    agosto = [f for f in barrer(cfg, date(2026, 7, 20), date(2026, 8, 10))]
    assert agosto, "debe generar el vencimiento de agosto"
    assert all(f < date(2026, 8, 1) for f in agosto), \
        f"el ajuste anterior debe mover hacia atrás, y dio {agosto}"


def test_las_dos_banderas_a_la_vez_gana_SIGUIENTE_y_conviene_saberlo():
    """Documenta el comportamiento cuando llegan las dos banderas.

    La interfaz debe impedir ese estado —y por eso pasó a ser un selector de
    tres opciones—, pero si una configuración vieja lo trae, gana `siguiente`
    por el `elif` del motor. Queda escrito para que nadie lo descubra
    depurando.
    """
    cfg = mensual(dia_corte=1, ajuste_habil=True, ajuste_habil_anterior=True)
    fechas = barrer(cfg, date(2026, 7, 20), date(2026, 8, 10))
    assert fechas and all(f >= date(2026, 8, 1) for f in fechas)


# --------------------------------------------------------------------------
# Otras periodicidades
# --------------------------------------------------------------------------

def test_ciclico_cada_siete_dias_genera_lo_esperado():
    cfg = {"modo": "CICLICO", "intervalo": 7, "fecha_inicio": "2026-01-05"}
    fechas = barrer(cfg, date(2026, 1, 1), date(2026, 3, 1))
    assert fechas[0] == date(2026, 1, 5)
    assert all((b - a).days == 7 for a, b in zip(fechas, fechas[1:]))


def test_antes_de_la_fecha_de_inicio_no_genera_nada():
    """Una obligación que empieza en marzo no puede producir tareas en enero."""
    cfg = mensual(dia_corte=15, fecha_inicio="2026-03-01")
    assert barrer(cfg, date(2026, 1, 1), date(2026, 2, 28)) == []


def test_semanal_genera_los_dias_pedidos():
    cfg = {"modo": "SEMANAL", "dias_semana": [0, 3]}  # lunes y jueves
    fechas = barrer(cfg, date(2026, 1, 5), date(2026, 1, 18))
    assert {f.weekday() for f in fechas} == {0, 3}
    assert len(fechas) == 4


def test_semanal_con_varios_dias_y_ajuste_NO_duplica_la_OBLIGACION():
    """Con lunes y martes marcados y plazo de gracia, un lunes festivo se
    entrega el martes — y ese martes ya tenía el suyo.

    Son DOS obligaciones distintas que comparten día de entrega, no una
    duplicada: la del lunes y la del martes. Lo que no puede repetirse es el día
    NOMINAL, que es lo que identifica cada obligación.

    Antes esto sí era un duplicado real —doce días al año— porque la obligación
    se identificaba por la fecha ya ajustada y las dos se volvían indistinguibles.
    """
    cfg = {"modo": "SEMANAL", "dias_semana": [0, 1], "mover_habil": True,
           "fecha_inicio": "2026-01-01"}
    venc = vencimientos(cfg, date(2026, 1, 1), date(2026, 12, 31))
    nominales = [v.nominal for v in venc]
    assert len(nominales) == len(set(nominales)), "una obligación aparece dos veces"

    # Y que el caso existe de verdad: si nunca coincidieran, la prueba no
    # estaría comprobando nada.
    limites = [v.limite for v in venc]
    assert len(limites) > len(set(limites)), (
        "ningún festivo hizo coincidir dos entregas; la prueba no mide nada")


def test_semanal_SI_aplica_el_ajuste_anterior():
    """El motor solo leía `mover_habil`: quien pedía «anterior» —la dirección
    segura para un plazo legal— no obtenía ningún ajuste."""
    con = barrer({"modo": "SEMANAL", "dias_semana": [0],
                  "mover_habil_anterior": True, "fecha_inicio": "2026-01-01"},
                 date(2026, 1, 1), date(2026, 12, 31))
    sin = barrer({"modo": "SEMANAL", "dias_semana": [0],
                  "fecha_inicio": "2026-01-01"},
                 date(2026, 1, 1), date(2026, 12, 31))
    assert con != sin, "el ajuste anterior debe cambiar algo"


def test_semanal_las_dos_direcciones_son_simetricas():
    """El 12 de enero de 2026 es lunes festivo. «Anterior» debe caer ANTES y
    «siguiente» DESPUÉS: si las dos movieran en el mismo sentido, elegir no
    serviría de nada."""
    base = {"modo": "SEMANAL", "dias_semana": [0], "fecha_inicio": "2026-01-01"}
    ventana = (date(2026, 1, 8), date(2026, 1, 16))
    atras = barrer({**base, "mover_habil_anterior": True}, *ventana)
    alante = barrer({**base, "mover_habil": True}, *ventana)
    assert atras and alante
    assert max(atras) < date(2026, 1, 12) < min(alante)


def test_semanal_la_vigencia_se_mide_contra_el_dia_NOMINAL():
    """La obligación nace el día que fija la regla, no el día en que se entrega.

    Con entrega anticipada, el día de entrega PUEDE caer antes del inicio de
    vigencia — y eso no borra la obligación: significa que hay que entregarla
    antes. Exigir que ambas fechas estuvieran dentro de vigencia era lo que hacía
    desaparecer obligaciones reales.
    """
    cfg = {"modo": "SEMANAL", "dias_semana": [0], "mover_habil_anterior": True,
           "fecha_inicio": "2026-01-12"}
    venc = vencimientos(cfg, date(2026, 1, 1), date(2026, 1, 31))
    assert venc, "no generó nada"
    assert all(v.nominal >= date(2026, 1, 12) for v in venc), \
        [v.nominal for v in venc]


# --------------------------------------------------------------------------
# La vista previa: debe coincidir con lo que el motor genera de verdad
# --------------------------------------------------------------------------

def test_la_vista_previa_coincide_con_el_motor():
    """Una vista previa que muestre fechas distintas de las que el sistema luego
    genera sería peor que no tenerla: nadie sabría cuál de las dos está mal."""
    from app.services.preview_calendario import proximos

    cfg = mensual(dia_corte=1, ajuste_habil_anterior=True)
    vista, aviso = proximos(cfg, cuantos=6, desde=date(2026, 1, 1))
    assert aviso == "", aviso

    reales = barrer(cfg, date(2026, 1, 1), date(2026, 12, 31))[:6]
    assert [v.fecha for v in vista] == reales


def test_la_vista_previa_dice_LAS_DOS_fechas():
    """«Vence el 1 de enero» y «hay que entregarlo antes del 31 de diciembre» son
    dos datos, y quien crea la obligación necesita los dos.

    Antes solo existía la fecha ajustada, así que la vista previa mostraba un
    31 de diciembre suelto para una regla que decía «día 1» — y había que
    deducir el resto.
    """
    from app.services.preview_calendario import proximos

    vista, _ = proximos(mensual(dia_corte=1, ajuste_habil_anterior=True),
                        cuantos=6, desde=date(2026, 1, 1))
    movidos = [v for v in vista if v.se_ajusto]
    assert movidos, "con día 1 y entrega anticipada, alguno debe adelantarse"
    for v in movidos:
        assert v.nominal.day == 1, "el día NOMINAL siempre es el que fija la regla"
        assert v.limite < v.nominal, "anticipada es hacia atrás"
        texto = v.describir()
        assert "Vence" in texto and "entregar antes de" in texto, texto


def test_la_vista_previa_distingue_gracia_de_anticipacion():
    """Las dos mueven la fecha de entrega, pero solo una amplía el plazo.

    Si el texto fuera el mismo —«movido a»— quien lee no puede saber si entregar
    ese día es cumplir o llegar tarde.
    """
    from app.services.preview_calendario import proximos

    gracia, _ = proximos(mensual(dia_corte=1, ajuste_habil=True),
                         cuantos=6, desde=date(2026, 1, 1))
    antic, _ = proximos(mensual(dia_corte=1, ajuste_habil_anterior=True),
                        cuantos=6, desde=date(2026, 1, 1))

    textos_gracia = [v.describir() for v in gracia if v.se_ajusto]
    textos_antic = [v.describir() for v in antic if v.se_ajusto]
    assert textos_gracia and textos_antic

    assert all("plazo hasta" in t for t in textos_gracia), textos_gracia
    assert all("entregar antes de" in t for t in textos_antic), textos_antic
    assert not any("plazo hasta" in t for t in textos_antic), (
        "la anticipación NO concede plazo: decirlo así sería falso")


def test_una_regla_que_no_genera_nada_LO_DICE():
    """Devolver una lista vacía sin explicación dejaría al usuario creyendo que
    configuró algo válido (directiva 20)."""
    from app.services.preview_calendario import proximos

    vacia, aviso = proximos({"modo": "ESPECIFICO", "meses": [],
                             "dia_corte": 15, "fecha_inicio": "2026-01-01"},
                            desde=date(2026, 1, 1))
    assert vacia == []
    assert "no genera ningún vencimiento" in aviso


def test_la_vista_previa_respeta_una_fecha_de_inicio_futura():
    """Si la regla empieza en 2027, barrer desde hoy no encontraría nada y
    parecería que no genera vencimientos."""
    from app.services.preview_calendario import proximos

    vista, aviso = proximos(mensual(dia_corte=10, fecha_inicio="2027-06-01"),
                            cuantos=3, desde=date(2026, 1, 1))
    assert aviso == "", aviso
    assert vista and vista[0].fecha >= date(2027, 6, 1)


# --------------------------------------------------------------------------
# El selector de ajuste: que el estado ambiguo no se pueda ni expresar
# --------------------------------------------------------------------------

def test_el_selector_tiene_exactamente_tres_opciones():
    """Tres respuestas posibles a «¿y si cae en día no hábil?», y una sola a la
    vez. Antes eran dos casillas que se podían marcar juntas."""
    from app.ui.reports_manager import AJUSTE_HABIL
    assert set(AJUSTE_HABIL) == {"ninguno", "siguiente", "anterior"}


@pytest.mark.parametrize("ajuste,esperado", [
    ("ninguno",   {"ajuste_habil": False, "ajuste_habil_anterior": False}),
    ("siguiente", {"ajuste_habil": True,  "ajuste_habil_anterior": False}),
    ("anterior",  {"ajuste_habil": False, "ajuste_habil_anterior": True}),
])
def test_el_selector_produce_banderas_sin_ambiguedad(ajuste, esperado):
    """Nunca las dos en verdadero: es el estado que el motor resolvía en
    silencio a favor de «siguiente»."""
    from app.ui.reports_manager import _banderas_desde_ajuste
    obtenido = _banderas_desde_ajuste(ajuste, "ajuste_habil", "ajuste_habil_anterior")
    assert obtenido == esperado
    assert not (obtenido["ajuste_habil"] and obtenido["ajuste_habil_anterior"])


def test_siempre_se_emiten_LAS_DOS_banderas():
    """Omitir la que va en falso dejaría el valor anterior si alguien fusiona
    diccionarios, y una obligación cambiaría de comportamiento sin que nadie la
    tocara."""
    from app.ui.reports_manager import _banderas_desde_ajuste
    for a in ("ninguno", "siguiente", "anterior"):
        assert set(_banderas_desde_ajuste(a, "x", "y")) == {"x", "y"}


@pytest.mark.parametrize("siguiente,anterior,esperado", [
    (False, False, "ninguno"),
    (True,  False, "siguiente"),
    (False, True,  "anterior"),
    (True,  True,  "siguiente"),   # config vieja ambigua: se muestra lo que el motor HACE
])
def test_al_abrir_un_informe_viejo_se_muestra_lo_que_el_motor_hace(siguiente, anterior, esperado):
    """Una configuración anterior al selector puede traer las dos banderas. Se
    muestra `siguiente` porque es lo que el `elif` del motor ejecuta — enseñar
    otra cosa haría creer al usuario que el sistema se comporta como no lo hace."""
    from app.ui.reports_manager import _ajuste_desde_banderas
    assert _ajuste_desde_banderas(siguiente, anterior) == esperado


def test_la_regla_que_construye_el_formulario_es_la_que_ve_el_motor():
    """`_construir_regla` la comparten `guardar()` y la vista previa. Si
    divergieran, la vista previa mostraría fechas que el sistema no genera."""
    from app.ui.reports_manager import _construir_regla
    from app.services.preview_calendario import proximos

    datos = {
        "modo_tiempo": "ESPECIFICO",
        "fecha_inicio": "2026-01-01",
        "tiempo_especifico": {"meses_seleccionados": TODOS_LOS_MESES,
                              "dia_corte": 1, "es_ultimo_dia": False,
                              "ajuste": "anterior"},
    }
    regla = _construir_regla(datos)
    assert regla["ajuste_habil_anterior"] is True
    assert regla["ajuste_habil"] is False

    vista, aviso = proximos(regla, cuantos=6, desde=date(2026, 1, 1))
    assert aviso == ""
    reales = barrer(regla, date(2026, 1, 1), date(2026, 12, 31))[:6]
    assert [v.fecha for v in vista] == reales
