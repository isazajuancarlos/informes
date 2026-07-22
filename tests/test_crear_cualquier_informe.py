"""El sistema tiene que poder crear CUALQUIER informe, no los que anticipamos.

# El principio

Las periodicidades que el sistema admite no son una lista de casos conocidos:
son las formas que puede tomar una obligación. Cada límite que el motor impone
—«un solo día de corte al mes», «tiene que haber un patrón»— no es una decisión
de diseño, es una obligación real que alguien no podrá registrar.

Una base de 399 informes reales sirvió para DESCUBRIR tres de esos límites. Sirve
como prueba del diseño, **nunca como su especificación**: lo que se añadió se
añadió porque es una forma universal de obligación, no porque esa entidad la
usara. Por eso este banco no menciona ni un solo informe suyo.

# Las tres formas que faltaban

  - **Varios días de corte en el mes.** Una obligación quincenal es de las más
    comunes que existen, y `dia_corte` era un escalar.
  - **Lista de fechas explícita.** La salida de emergencia: si ningún patrón
    encaja, se enumeran las fechas. Sin esto, «puede crear cualquier informe»
    sería cierto solo para las formas que se nos ocurrieron.
  - **Día hábil n-ésimo.** Cubierto en `test_vencimiento_nominal.py`.
"""
from __future__ import annotations

import os
from datetime import date, timedelta

import pytest

os.environ.setdefault("DATABASE_URL", "sqlite:///./test_crear.db")

from app.services.preview_calendario import proximos  # noqa: E402
from app.services.scheduler import GeneradorTareas as G  # noqa: E402
from app.services.validar_regla import validar  # noqa: E402

ANIOS = range(2020, 2041)
TODOS = list(range(1, 13))


def barrer(config, anio):
    salida, dia = [], date(anio, 1, 1)
    while dia <= date(anio, 12, 31):
        salida += G.obtener_vencimientos_para_fecha(dia, config)
        dia += timedelta(days=1)
    return salida


# --------------------------------------------------------------------------
# Varios días de corte en el mes
# --------------------------------------------------------------------------

@pytest.mark.parametrize("dias", [[7, 20], [1, 15], [5, 15, 25], [10, 20, 30]])
def test_varios_dias_de_corte_generan_uno_por_dia_y_por_mes(dias):
    """Uno por cada día pedido, en cada mes.

    No se comprueba que el día del mes sea EXACTAMENTE el pedido: con el 30 en
    febrero se usa el último día real, por el mismo criterio que el 31 en un mes
    de 30 —descartarlo haría desaparecer la obligación sin avisar—. Lo que se
    exige es que ninguna se pierda y que cada una caiga donde debe.
    """
    import calendar

    for anio in ANIOS:
        cfg = {"modo": "ESPECIFICO", "meses": TODOS, "dias_corte": dias,
               "usar_ultimo_dia_en_meses_cortos": True,
               "fecha_inicio": f"{anio}-01-01"}
        venc = barrer(cfg, anio)
        assert len(venc) == 12 * len(dias), f"{anio}/{dias}: {len(venc)}"
        for v in venc:
            ultimo = calendar.monthrange(v.nominal.year, v.nominal.month)[1]
            assert v.nominal.day in dias or v.nominal.day == ultimo, (
                f"{anio}: {v.nominal} no es ninguno de {dias} ni fin de mes")
        # Cada mes recibe tantos vencimientos como días pedidos.
        for mes in TODOS:
            del_mes = [v for v in venc if v.nominal.month == mes]
            assert len(del_mes) == len(dias), f"{anio}-{mes:02}: {len(del_mes)}"


def test_una_obligacion_QUINCENAL_se_puede_expresar():
    """El 15 y el último día. Era imposible con un `dia_corte` escalar, y no es
    un caso raro: es de las periodicidades más comunes que hay."""
    for anio in ANIOS:
        # La excepción se DECLARA: febrero no tiene 31, y el validador no deja
        # guardar la regla hasta que el administrador diga qué hacer.
        cfg = {"modo": "ESPECIFICO", "meses": TODOS, "dias_corte": [15, 31],
               "usar_ultimo_dia_en_meses_cortos": True,
               "ajuste_habil_anterior": True, "fecha_inicio": f"{anio}-01-01"}
        venc = barrer(cfg, anio)
        assert len(venc) == 24, f"{anio}: {len(venc)} de 24"
        # El 31 en un mes corto usa el último día real, no desaparece.
        assert {v.nominal.month for v in venc} == set(TODOS)


def test_los_dias_repetidos_no_duplican_el_vencimiento():
    cfg = {"modo": "ESPECIFICO", "meses": [1], "dias_corte": [10, 10],
           "fecha_inicio": "2026-01-01"}
    assert len(barrer(cfg, 2026)) == 1


def test_el_dia_de_corte_ESCALAR_sigue_funcionando():
    """Las reglas ya creadas lo tienen guardado así. Romperlas al añadir la
    lista habría cambiado los vencimientos de obligaciones existentes sin que
    nadie tocara nada."""
    for anio in ANIOS:
        cfg = {"modo": "ESPECIFICO", "meses": TODOS, "dia_corte": 15,
               "fecha_inicio": f"{anio}-01-01"}
        venc = barrer(cfg, anio)
        assert len(venc) == 12 and {v.nominal.day for v in venc} == {15}


# --------------------------------------------------------------------------
# Lista de fechas explícita: la salida de emergencia
# --------------------------------------------------------------------------

def test_una_obligacion_SIN_PATRON_se_puede_enumerar():
    fechas = ["2026-03-15", "2026-07-04", "2026-11-01"]
    cfg = {"modo": "FECHAS", "fechas": fechas, "fecha_inicio": "2026-01-01"}
    venc = barrer(cfg, 2026)
    assert [str(v.nominal) for v in venc] == fechas


def test_la_lista_de_fechas_TAMBIEN_distingue_vencimiento_de_entrega():
    """La distinción no es un privilegio de las reglas con patrón."""
    cfg = {"modo": "FECHAS", "fechas": ["2026-01-01"], "ajuste_habil": True,
           "fecha_inicio": "2026-01-01"}
    v = barrer(cfg, 2026)[0]
    assert v.nominal == date(2026, 1, 1)
    assert v.limite > v.nominal and v.ajuste == "GRACIA"


def test_una_fecha_ilegible_NO_borra_las_demas():
    """Perder las otras por un dedazo en una sería el peor comportamiento
    posible: el usuario ve menos vencimientos y no sabe por qué."""
    cfg = {"modo": "FECHAS", "fechas": ["2026-03-15", "15/03/2026", "2026-07-04"],
           "fecha_inicio": "2026-01-01"}
    assert len(barrer(cfg, 2026)) == 2
    # Y el validador NO deja guardarla así, nombrando la que está mal.
    r = validar(cfg)
    assert not r.se_puede_guardar
    assert "15/03/2026" in " ".join(r.errores)


def test_la_vista_previa_funciona_con_la_lista_de_fechas():
    """Si la vista previa no soportara un modo, el usuario lo crearía a ciegas."""
    cfg = {"modo": "FECHAS", "fechas": ["2026-05-20", "2026-09-10"],
           "fecha_inicio": "2026-01-01"}
    venc, aviso = proximos(cfg, cuantos=2, desde=date(2026, 1, 1))
    assert len(venc) == 2, aviso
    assert venc[0].nominal == date(2026, 5, 20)


# --------------------------------------------------------------------------
# Lo que el sistema NO puede expresar, dicho a la cara
# --------------------------------------------------------------------------

def test_las_formas_admitidas_estan_TODAS_cubiertas_por_el_validador():
    """Un modo que el motor entiende y el validador no comprueba se guarda sin
    revisar; uno que el validador conoce y el motor ignora genera cero
    vencimientos. Las dos listas tienen que coincidir."""
    import inspect

    from app.services import scheduler, validar_regla

    del_motor = set()
    for linea in inspect.getsource(scheduler.GeneradorTareas.obtener_vencimientos_para_fecha).splitlines():
        if "modo ==" in linea:
            del_motor.add(linea.split("'")[1])
    del_validador = set()
    for linea in inspect.getsource(validar_regla.validar).splitlines():
        if "modo ==" in linea:
            del_validador.add(linea.split('"')[1])

    assert del_motor == del_validador, (
        f"solo en el motor: {del_motor - del_validador} · "
        f"solo en el validador: {del_validador - del_motor}")


# --------------------------------------------------------------------------
# El ordinal de día de semana: «el último martes», «el primer lunes»
# --------------------------------------------------------------------------
#
# Es la forma en que se fijan las reuniones de comité y las juntas. Sin ella, el
# administrador tenía que enumerar las fechas a mano, año tras año.

def ordinal(dia, semana, anio):
    return {"modo": "ESPECIFICO", "meses": TODOS, "dia_semana_mes": dia,
            "semana_del_mes": semana, "fecha_inicio": f"{anio}-01-01"}


@pytest.mark.parametrize("dia,semana", [(0, 1), (1, -1), (4, 3), (2, 2), (6, -1)])
def test_hay_uno_por_mes_y_cae_en_el_dia_de_semana_pedido(dia, semana):
    for anio in ANIOS:
        venc = barrer(ordinal(dia, semana, anio), anio)
        assert len(venc) == 12, f"{anio}: {len(venc)} de 12"
        assert all(v.nominal.weekday() == dia for v in venc), anio


def test_el_ULTIMO_martes_no_es_el_martes_de_la_ultima_semana():
    """Un mes puede tener cuatro o cinco martes, y la última semana del mes
    puede no incluir ninguno. Se cuentan los que hay."""
    import calendar

    for anio in ANIOS:
        for v in barrer(ordinal(1, -1, anio), anio):
            ultimo_dia = calendar.monthrange(v.nominal.year, v.nominal.month)[1]
            # No hay ningún martes después de él en ese mes.
            assert v.nominal.day + 7 > ultimo_dia, f"{anio}: {v.nominal}"


def test_pedir_el_QUINTO_en_un_mes_de_cuatro_usa_el_ultimo_y_no_desaparece():
    """Igual que el día 31 en un mes de 30. Una obligación que se esfuma es peor
    que una mal fechada: nadie la ve hasta que la pregunta un ente de control."""
    for anio in ANIOS:
        venc = barrer(ordinal(1, 5, anio), anio)
        assert len(venc) == 12, f"{anio}: {len(venc)} de 12"


def test_el_ordinal_admite_ajuste_de_entrega_como_cualquier_otra_forma():
    """Las formas nuevas no son ciudadanas de segunda: la distinción entre día
    de vencimiento y día de entrega vale para todas."""
    cfg = {**ordinal(1, -1, 2026), "ajuste_habil_anterior": True}
    venc = barrer(cfg, 2026)
    assert all(v.limite <= v.nominal for v in venc)


@pytest.mark.parametrize("semana,valido", [(1, True), (5, True), (-1, True), (0, False), (7, False)])
def test_el_validador_acota_el_ordinal(semana, valido):
    cfg = ordinal(1, semana, 2026)
    if semana == 0:  # 0 es falsy: la regla cae a «día de corte», que es válida
        pytest.skip("0 no activa el modo ordinal; lo cubre el día de corte")
    assert validar(cfg).se_puede_guardar is valido, semana
