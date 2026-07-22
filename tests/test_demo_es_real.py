"""La demo tiene que ser el producto, no una maqueta del producto.

# Lo que este banco impide

`demo_empresa_privada.py` es lo que se le enseña a un cliente. Hasta ahora era un
teatro, y no de forma evidente:

  - Sus reglas usaban un esquema propio (`freq`, `bymonthday`, `byweekday`) que
    el planificador de producción **ignora**. Medido: generaban **cero**
    vencimientos, y el validador las habría rechazado al intentar guardarlas.
  - Las fechas las calculaba un **segundo motor de calendario**
    (`MotorReglasDinamicas.generar_calendario`), distinto del que corre en
    producción.
  - Las entregas se insertaban a mano en la base, así que la pantalla se veía
    perfecta.

El resultado: una demostración impecable de un sistema que, instalado tal cual,
no habría producido ni una sola tarea. Es la misma familia de fallo que la vista
previa con su propia copia de la lógica — con el agravante de que aquí el
engañado es el cliente.

# Qué se comprueba

Que cada regla de la demo **sobrevive al camino real**: la acepta el validador y
el motor de producción le genera vencimientos. Nada de esto mira la pantalla.
"""
from __future__ import annotations

import os
from datetime import date, timedelta

import pytest

os.environ.setdefault("DATABASE_URL", "sqlite:///./test_demo.db")
os.environ.setdefault("DEPLOYMENT_PROFILE", "standalone")

from app.services.scheduler import GeneradorTareas as G  # noqa: E402
from app.services.validar_regla import validar  # noqa: E402
from demo_empresa_privada import OBLIGACIONES  # noqa: E402

CASOS = [(o["nombre"], o["recurrencia"]) for o in OBLIGACIONES]
CADENCIAS = [(o["nombre"], o["recurrencia"], o["vencimientos_al_ano"])
             for o in OBLIGACIONES if "vencimientos_al_ano" in o]


def barrer(regla, desde, hasta):
    salida, dia = [], desde
    while dia <= hasta:
        salida += G.obtener_vencimientos_para_fecha(dia, regla)
        dia += timedelta(days=1)
    return salida


def test_la_demo_define_obligaciones():
    """Si la lista quedara vacía, todo lo demás pasaría sin comprobar nada."""
    assert len(CASOS) >= 8, len(CASOS)


@pytest.mark.parametrize("nombre,regla", CASOS)
def test_cada_regla_de_la_demo_la_ACEPTARIA_el_formulario(nombre, regla):
    """Si el validador la rechaza, el cliente no podría crear esa obligación en
    el sistema que se le acaba de enseñar."""
    r = validar(regla)
    assert r.se_puede_guardar, f"{nombre}: {r.errores}"


@pytest.mark.parametrize("nombre,regla", CASOS)
def test_cada_regla_de_la_demo_genera_vencimientos_EN_EL_MOTOR_REAL(nombre, regla):
    """El motor de producción, no el de la demo. Es toda la diferencia."""
    venc = barrer(regla, date(2026, 1, 1), date(2026, 12, 31))
    assert venc, f"{nombre}: la regla no genera NINGÚN vencimiento"


@pytest.mark.parametrize("nombre,regla", CASOS)
def test_ninguna_regla_de_la_demo_usa_el_esquema_ABANDONADO(nombre, regla):
    """`freq`, `bymonthday` y `byweekday` son de un motor que producción no usa.

    Se comprueba sobre la regla y no sobre el texto del archivo: lo que importa
    es lo que se guarda en `recurrencia_json`, que es lo que el planificador lee.
    """
    abandonadas = {"freq", "bymonthday", "byweekday", "interval", "ajuste"}
    intrusas = abandonadas & set(regla)
    assert not intrusas, f"{nombre}: usa el esquema viejo {sorted(intrusas)}"


@pytest.mark.parametrize("nombre,regla,esperados", CADENCIAS)
def test_la_periodicidad_es_la_DECLARADA_no_la_que_salga(nombre, regla, esperados):
    """Que genere «algo» no basta: una trimestral vuelta mensual pasaría.

    El número esperado lo declara la obligación en `vencimientos_al_ano`, aparte
    de la regla. La primera versión de esta prueba lo derivaba de la propia
    regla —`len(regla["meses"])`— así que al cambiar la regla cambiaba también
    lo esperado: era circular y no podía fallar. Se descubrió mutando una
    trimestral a mensual y viendo que el banco seguía verde.

    Dos afirmaciones independientes —la intención y la implementación— es lo
    único que permite detectar que se separan.
    """
    venc = barrer(regla, date(2026, 1, 1), date(2026, 12, 31))
    assert len(venc) == esperados, (
        f"{nombre}: la obligación declara {esperados} vencimientos al año y el "
        f"motor genera {len(venc)}")


def test_todas_las_obligaciones_declaran_su_periodicidad():
    """Sin la declaración, la prueba de arriba se saltaría en silencio."""
    faltan = [o["nombre"] for o in OBLIGACIONES if "vencimientos_al_ano" not in o]
    assert not faltan, f"sin `vencimientos_al_ano`: {faltan}"


def test_la_demo_ejercita_LAS_DOS_direcciones_del_ajuste():
    """Una demo con un solo tipo de ajuste no enseña la distinción entre
    entrega anticipada y plazo de gracia, que es lo que separa este producto de
    un calendario cualquiera."""
    ajustes = set()
    for _, regla in CASOS:
        for v in barrer(regla, date(2026, 1, 1), date(2026, 12, 31)):
            if v.se_movio:
                ajustes.add(v.ajuste)
    assert ajustes == {"ANTICIPADO", "GRACIA"}, ajustes


def test_el_segundo_motor_de_calendario_NO_ha_vuelto():
    """`generar_calendario` era una implementación paralela del núcleo, con otro
    esquema de regla y un docstring que se declaraba «EL CORAZÓN DEL SISTEMA»
    sin que producción lo llamara nunca.

    Dos motores para lo mismo garantizan que uno de los dos esté mal y que nadie
    sepa cuál. Esta prueba existe para que no reaparezca por comodidad.
    """
    from app.services.calendar_engine import MotorReglasDinamicas

    assert not hasattr(MotorReglasDinamicas, "generar_calendario"), (
        "volvió el segundo motor de calendario: las fechas deben salir de "
        "GeneradorTareas.obtener_vencimientos_para_fecha")
