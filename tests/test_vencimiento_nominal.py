"""El día de vencimiento y el día de entrega son dos cosas distintas.

# Qué distingue este banco

Un informe anual con fecha fija el 1 de enero **vence el 1 de enero**. Siempre.
Que ese día sea festivo no mueve la obligación: mueve la entrega. Y hay dos
maneras de moverla, que no significan lo mismo:

  - **Entrega anticipada** (día hábil anterior). No concede plazo: es una regla
    de control interno para que alguien lo revise cuando hay quien trabaje. El
    vencimiento sigue siendo el 1 de enero.
  - **Plazo de gracia** (día hábil siguiente). Sí amplía el plazo: entregar ese
    día NO es incumplimiento.

Confundirlas costaba tres defectos a la vez, y los tres eran invisibles:

  1. La obligación cambiaba de PERIODO. El vencimiento de enero con entrega
     anticipada «pertenecía» al 31 de diciembre, y por tanto al año anterior.
  2. Con vigencia desde el 1 de enero, ese vencimiento caía fuera y la
     obligación **desaparecía**: sin tarea, sin alerta y sin rastro.
  3. Nadie podía saber si entregar el día límite era cumplir o llegar tarde.

# Y por qué se prueban 21 años

`informes` está para funcionar de manera perpetua. Una regla se escribe una vez
y tiene que seguir siendo correcta en 2030 y en 2099, con otros festivos y otros
días de la semana. Un año concreto es un EJEMPLO; si una prueba solo pasa en
2026, no ha probado la regla, ha probado el almanaque de 2026.
"""
from __future__ import annotations

import os
from datetime import date

import pytest

os.environ.setdefault("DATABASE_URL", "sqlite:///./test_nominal.db")

from app.services.scheduler import (  # noqa: E402
    AJUSTE_ANTICIPADO,
    AJUSTE_GRACIA,
    AJUSTE_NINGUNO,
    GeneradorTareas as G,
)

#: Rango de comprobación. Cubre bisiestos, los siete días de la semana para el
#: 1 de enero y veinte configuraciones distintas de festivos móviles.
ANIOS = range(2020, 2041)

AJUSTES = {
    "sin ajuste": {},
    "entrega anticipada": {"ajuste_habil_anterior": True},
    "plazo de gracia": {"ajuste_habil": True},
}


def anual_primero_de_enero(anio, **extra):
    """El caso del que hablamos: informe anual, fecha fija 1 de enero."""
    return {"modo": "ESPECIFICO", "meses": [1], "dia_corte": 1,
            "fecha_inicio": f"{anio}-01-01", **extra}


def unico(config, dia):
    v = G.obtener_vencimientos_para_fecha(dia, config)
    assert len(v) == 1, f"se esperaba un vencimiento el {dia}, hubo {len(v)}"
    return v[0]


# --------------------------------------------------------------------------
# El día de vencimiento NO se mueve
# --------------------------------------------------------------------------

@pytest.mark.parametrize("nombre,extra", list(AJUSTES.items()))
def test_el_dia_de_vencimiento_es_el_de_la_regla_en_CUALQUIER_anio(nombre, extra):
    """El 1 de enero es festivo en Colombia todos los años. La obligación
    vence igual el 1 de enero, con ajuste y sin él."""
    for anio in ANIOS:
        cfg = anual_primero_de_enero(anio, **extra)
        v = unico(cfg, date(anio, 1, 1))
        assert v.nominal == date(anio, 1, 1), f"{nombre} en {anio}: {v.nominal}"


@pytest.mark.parametrize("nombre,extra", list(AJUSTES.items()))
def test_la_obligacion_NUNCA_desaparece(nombre, extra):
    """El caso que se perdía: vigencia desde el 1 de enero y entrega anticipada.

    La entrega cae el 31 de diciembre ANTERIOR, fuera de vigencia. Antes eso
    borraba la obligación entera. Ahora la obligación nace el 1 de enero —dentro
    de vigencia— y lo que queda fuera es su entrega, que es otra cosa.
    """
    for anio in ANIOS:
        cfg = anual_primero_de_enero(anio, **extra)
        assert G.obtener_vencimientos_para_fecha(date(anio, 1, 1), cfg), \
            f"{nombre} en {anio}: la obligación del 1 de enero se perdió"


# --------------------------------------------------------------------------
# Cada ajuste va en su dirección, y solo uno concede plazo
# --------------------------------------------------------------------------

def test_la_entrega_anticipada_va_HACIA_ATRAS_y_no_concede_plazo():
    for anio in ANIOS:
        cfg = anual_primero_de_enero(anio, ajuste_habil_anterior=True)
        v = unico(cfg, date(anio, 1, 1))
        assert v.limite < v.nominal, f"{anio}: {v.limite} no está antes de {v.nominal}"
        assert v.ajuste == AJUSTE_ANTICIPADO
        assert v.es_anticipado and not v.es_gracia, (
            f"{anio}: adelantar la entrega NO es conceder plazo")


def test_el_plazo_de_gracia_va_HACIA_DELANTE_y_SI_concede_plazo():
    for anio in ANIOS:
        cfg = anual_primero_de_enero(anio, ajuste_habil=True)
        v = unico(cfg, date(anio, 1, 1))
        assert v.limite > v.nominal, f"{anio}: {v.limite} no está después de {v.nominal}"
        assert v.ajuste == AJUSTE_GRACIA
        assert v.es_gracia, f"{anio}: entregar el día límite debe ser cumplir"


def test_sin_ajuste_las_dos_fechas_coinciden():
    for anio in ANIOS:
        v = unico(anual_primero_de_enero(anio), date(anio, 1, 1))
        assert v.nominal == v.limite and v.ajuste == AJUSTE_NINGUNO
        assert not v.se_movio


def test_los_dos_ajustes_NUNCA_dan_el_mismo_resultado_cuando_el_dia_no_es_habil():
    """Si dieran lo mismo, la elección del usuario no serviría de nada."""
    for anio in ANIOS:
        antes = unico(anual_primero_de_enero(anio, ajuste_habil_anterior=True),
                      date(anio, 1, 1))
        despues = unico(anual_primero_de_enero(anio, ajuste_habil=True),
                        date(anio, 1, 1))
        assert antes.limite != despues.limite, f"{anio}: las dos opciones coinciden"
        assert antes.limite < antes.nominal < despues.limite


# --------------------------------------------------------------------------
# Perpetuidad: la regla se escribe una vez y sigue siendo correcta
# --------------------------------------------------------------------------

def test_el_dia_nominal_no_depende_del_calendario_de_festivos():
    """Veintiún calendarios distintos, el mismo día nominal.

    Es la propiedad que hace que una regla escrita hoy siga valiendo dentro de
    diez años: lo que cambia año a año son los festivos, y el día nominal no los
    mira.
    """
    nominales = set()
    for anio in ANIOS:
        for extra in AJUSTES.values():
            v = unico(anual_primero_de_enero(anio, **extra), date(anio, 1, 1))
            nominales.add((v.nominal.month, v.nominal.day))
    assert nominales == {(1, 1)}, nominales


def test_el_dia_de_entrega_SI_cambia_con_los_festivos_de_cada_anio():
    """El contrapunto del anterior. Si la entrega tampoco cambiara, el ajuste no
    estaría aplicándose y la prueba de arriba pasaría por la razón equivocada."""
    limites = {unico(anual_primero_de_enero(a, ajuste_habil_anterior=True),
                     date(a, 1, 1)).limite.weekday() for a in ANIOS}
    assert len(limites) > 1, (
        "el día de entrega salió siempre igual: el ajuste no se está aplicando")


@pytest.mark.parametrize("anio", [2027, 2035, 2050, 2099])
def test_una_regla_mensual_sigue_funcionando_en_anios_LEJANOS(anio):
    """Los festivos se derivan de la ley, no de una tabla cargada a mano. Sin
    esto, el sistema dejaría de ajustar fechas el año en que se acabara la tabla
    — y lo haría en silencio, que es la forma cara de fallar."""
    cfg = {"modo": "ESPECIFICO", "meses": list(range(1, 13)), "dia_corte": 15,
           "ajuste_habil_anterior": True, "fecha_inicio": f"{anio}-01-01"}
    encontrados = []
    dia = date(anio, 1, 1)
    while dia <= date(anio, 12, 31):
        encontrados += G.obtener_vencimientos_para_fecha(dia, cfg)
        dia = date.fromordinal(dia.toordinal() + 1)
    assert len(encontrados) == 12, f"{anio}: {len(encontrados)} de 12"
    assert {v.nominal.day for v in encontrados} == {15}


# --------------------------------------------------------------------------
# Sin calendario de festivos se FALLA, no se sigue adelante
# --------------------------------------------------------------------------

def test_un_pais_sin_calendario_LEVANTA_en_vez_de_asumir_que_no_hay_festivos():
    """Antes caía a un calendario vacío imprimiendo un aviso.

    El resultado era un sistema que parecía funcionar: cada festivo contaba como
    hábil, ningún vencimiento se ajustaba, y todas las fechas salían plausibles
    y equivocadas. Un `print` no es fallar.
    """
    from app.services.calendar_engine import MotorReglasDinamicas

    with pytest.raises(ValueError) as e:
        MotorReglasDinamicas(pais_iso="ZZ")
    assert "festivos" in str(e.value).lower()
    assert "PAIS_ISO" in str(e.value), "el error debe decir qué revisar"


def test_el_camino_sin_festivos_existe_pero_hay_que_PEDIRLO():
    """Que el defecto sea fallar no quita que el caso legítimo tenga salida."""
    from app.services.calendar_engine import MotorReglasDinamicas

    motor = MotorReglasDinamicas(pais_iso="ZZ", sin_festivos=True)
    assert motor._es_habil(date(2026, 1, 1)), "sin festivos, el 1 de enero es hábil"


def test_el_calendario_real_SI_conoce_los_festivos_de_anios_lejanos():
    """Discriminación del anterior: si el motor real tampoco tuviera festivos,
    las pruebas de ajuste pasarían sin ajustar nada."""
    from app.services.calendar_engine import MotorReglasDinamicas

    motor = MotorReglasDinamicas()
    for anio in (2026, 2050, 2099):
        assert not motor._es_habil(date(anio, 1, 1)), \
            f"el 1 de enero de {anio} debería ser festivo"


# --------------------------------------------------------------------------
# El día HÁBIL n-ésimo: «dentro de los primeros cinco días hábiles»
# --------------------------------------------------------------------------
#
# Es la forma más frecuente de fijar un plazo en la norma colombiana y NO se
# puede expresar moviendo una fecha. Medido sobre 399 informes reales de una
# entidad que opera: 41 de los 318 con periodicidad codificada —uno de cada
# ocho— la usan. Era el único hueco grande del motor.


def habil(n, anio):
    return {"modo": "ESPECIFICO", "meses": list(range(1, 13)),
            "dia_habil": n, "fecha_inicio": f"{anio}-01-01"}


def del_anio(config, anio):
    salida, dia = [], date(anio, 1, 1)
    while dia <= date(anio, 12, 31):
        salida += G.obtener_vencimientos_para_fecha(dia, config)
        dia = date.fromordinal(dia.toordinal() + 1)
    return salida


@pytest.mark.parametrize("n", [1, 3, 5, 10, 15])
def test_el_dia_habil_n_esimo_cae_SIEMPRE_en_dia_habil(n):
    """Por construcción ya es hábil, así que no hay ajuste que aplicarle."""
    from app.services.calendar_engine import MotorReglasDinamicas

    motor = MotorReglasDinamicas()
    for anio in ANIOS:
        for v in del_anio(habil(n, anio), anio):
            assert motor._es_habil(v.nominal), f"{anio}: {v.nominal} no es hábil"
            assert v.ajuste == AJUSTE_NINGUNO
            assert not v.se_movio


@pytest.mark.parametrize("n", [1, 3, 5, 10, 15])
def test_hay_UNO_por_mes_en_cualquier_anio(n):
    for anio in ANIOS:
        venc = del_anio(habil(n, anio), anio)
        assert len(venc) == 12, f"{anio}: {len(venc)} de 12"
        assert sorted({v.nominal.month for v in venc}) == list(range(1, 13))


def test_NO_es_lo_mismo_que_el_dia_N_ajustado_a_habil():
    """La distinción entera. Si diera igual, la dimensión sobraría.

    El 5º día hábil de enero de 2026 es el 8, porque el 1 es festivo. «Día 5
    ajustado» da el 5, que es lunes hábil. Tres días de diferencia, y esos tres
    días son un incumplimiento.
    """
    por_habiles = del_anio(habil(5, 2026), 2026)
    corridos = del_anio({"modo": "ESPECIFICO", "meses": list(range(1, 13)),
                         "dia_corte": 5, "ajuste_habil": True,
                         "fecha_inicio": "2026-01-01"}, 2026)
    assert [v.nominal for v in por_habiles] != [v.limite for v in corridos]
    assert por_habiles[0].nominal == date(2026, 1, 8), por_habiles[0].nominal


def test_el_primer_dia_habil_no_es_necesariamente_el_dia_1():
    """Enero empieza en festivo todos los años en Colombia."""
    for anio in ANIOS:
        primero = del_anio(habil(1, anio), anio)[0]
        assert primero.nominal.day > 1, f"{anio}: {primero.nominal}"


def test_si_el_mes_tiene_menos_habiles_se_usa_el_ULTIMO_y_no_desaparece():
    """Un mes tiene entre 18 y 23 días hábiles. Pedir el 23 en un mes de 18 no
    puede hacer que el vencimiento se esfume: eso sería una obligación que nadie
    ve. Se usa el último hábil, igual que el día 31 en un mes de 30."""
    from app.services.calendar_engine import MotorReglasDinamicas

    motor = MotorReglasDinamicas()
    for anio in ANIOS:
        venc = del_anio(habil(23, anio), anio)
        assert len(venc) == 12, f"{anio}: {len(venc)} de 12"
        for v in venc:
            assert motor._es_habil(v.nominal)


def test_el_dia_habil_manda_sobre_el_dia_de_corte():
    """Los dos puestos: gana el hábil, y el validador lo advierte."""
    from app.services.validar_regla import validar

    cfg = {"modo": "ESPECIFICO", "meses": [1], "dia_habil": 5, "dia_corte": 20,
           "fecha_inicio": "2026-01-01"}
    v = G.obtener_vencimientos_para_fecha(date(2026, 1, 8), cfg)
    assert v and v[0].nominal == date(2026, 1, 8)
    assert not G.obtener_vencimientos_para_fecha(date(2026, 1, 20), cfg)

    r = validar(cfg)
    assert r.se_puede_guardar
    assert any("no se usarán" in a for a in r.advertencias), r.advertencias


@pytest.mark.parametrize("n,valido", [(0, True), (1, True), (23, True), (24, False), (99, False)])
def test_el_validador_acota_el_dia_habil(n, valido):
    from app.services.validar_regla import validar

    cfg = {"modo": "ESPECIFICO", "meses": [1], "fecha_inicio": "2026-01-01"}
    if n:
        cfg["dia_habil"] = n
    else:
        cfg["dia_corte"] = 5
    assert validar(cfg).se_puede_guardar is valido, n
