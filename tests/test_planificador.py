"""El planificador escribiendo en la base, que es donde nada se probaba.

Todo el banco del calendario prueba el motor de fechas. Pero entre el motor y una
tarea que alguien ve hay un paso que no tenía ninguna prueba:
`generar_pendientes_del_dia`, que decide **cuándo aparece** la tarea, **con qué
fechas se guarda** y **si ya existía**. Ahí viven los fallos que no se ven
mirando fechas sueltas:

  - Una tarea con entrega anticipada que nace **después** de su propio día de
    entrega. El motor la calcula bien y el usuario la recibe tarde.
  - La misma obligación creada **dos veces** al correr el planificador dos días
    seguidos.
  - Dos obligaciones distintas colapsadas en **una** porque comparten día de
    entrega tras el ajuste.

Se simulan cientos de corridas día a día en vez de una sola, porque el
planificador es incremental: cada corrida arranca donde acabó la anterior, y el
defecto de las corridas repetidas no aparece nunca en la primera.
"""
from __future__ import annotations

import os
import sys
from datetime import date, datetime, timedelta

import pytest

os.environ.setdefault("DEPLOYMENT_PROFILE", "standalone")
os.environ["DATABASE_URL"] = "sqlite:///:memory:"
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from app.core.database import Base, SessionLocal, engine  # noqa: E402
from app.models.meta_tables import (  # noqa: E402
    ROL_EJECUTA, EntregaEsperada, Informe, Participacion, Usuario,
)
from app.services import scheduler as sched  # noqa: E402


@pytest.fixture
def db():
    Base.metadata.create_all(engine)
    s = SessionLocal()
    yield s
    s.close()
    Base.metadata.drop_all(engine)


def _informe_con_responsable(db, regla, nombre="Informe anual", correo="r@x.co"):
    """`responsables` es una propiedad derivada de las participaciones, no una
    lista donde se pueda insertar. Montarlo mal dejaba el informe sin nadie que
    lo ejecutara y el planificador no creaba nada — en silencio."""
    u = Usuario(nombre="Responsable", email=correo, password_hash="x")
    db.add(u)
    db.flush()
    i = Informe(nombre=nombre, activo=True, recurrencia_json=regla)
    db.add(i)
    db.flush()
    db.add(Participacion(informe=i, usuario=u, rol=ROL_EJECUTA))
    db.commit()
    assert i.responsables, "el informe quedó sin responsable: la prueba no mediría nada"
    return i


def _correr_dias(desde, hasta):
    """Corre el planificador una vez por cada día del rango, como en producción.

    Correrlo una sola vez sobre el rango entero no prueba lo mismo: el
    planificador es incremental y los duplicados solo aparecen al repetir.
    """
    dia = desde
    corridas = 0
    while dia <= hasta:
        sched.GeneradorTareas.generar_pendientes_del_dia(hoy=dia)
        corridas += 1
        dia += timedelta(days=1)
    return corridas


# --------------------------------------------------------------------------
# La tarea tiene que existir ANTES de su día de entrega
# --------------------------------------------------------------------------

def test_la_tarea_con_entrega_anticipada_NO_nace_ya_vencida(db):
    """Informe anual del 1 de enero con entrega el hábil anterior.

    La entrega vence el 31 de diciembre, un día ANTES del vencimiento nominal.
    Si el planificador solo mirase hasta hoy, la tarea aparecería el 1 de enero
    —cuando su plazo ya pasó— y el responsable la recibiría incumplida sin haber
    tenido ocasión de cumplirla.
    """
    _informe_con_responsable(db, {
        "modo": "ESPECIFICO", "meses": [1], "dia_corte": 1,
        "ajuste_habil_anterior": True, "fecha_inicio": "2025-12-01"})

    _correr_dias(date(2025, 12, 1), date(2025, 12, 31))

    tareas = db.query(EntregaEsperada).all()
    assert len(tareas) == 1, f"se esperaba una tarea, hay {len(tareas)}"
    t = tareas[0]
    assert t.fecha_vencimiento_nominal.date() == date(2026, 1, 1)
    assert t.fecha_limite_teorica.date() < date(2026, 1, 1), "la entrega se adelanta"
    assert t.fecha_programada.date() <= t.fecha_limite_teorica.date(), (
        f"la tarea apareció el {t.fecha_programada.date()}, después de su "
        f"límite {t.fecha_limite_teorica.date()}: nace vencida")


def test_con_plazo_de_gracia_la_tarea_aparece_el_dia_del_vencimiento(db):
    """El contrapunto: sin anticipación no se adelanta nada.

    Si el arreglo anterior hubiera adelantado TODAS las tareas, esta prueba lo
    delata. Adelantar de más también es un defecto: llena la bandeja de tareas
    que aún no tocan.
    """
    _informe_con_responsable(db, {
        "modo": "ESPECIFICO", "meses": [1], "dia_corte": 1,
        "ajuste_habil": True, "fecha_inicio": "2025-12-01"})

    _correr_dias(date(2025, 12, 1), date(2025, 12, 31))
    assert db.query(EntregaEsperada).count() == 0, (
        "con plazo de gracia no hay nada que adelantar a diciembre")

    _correr_dias(date(2026, 1, 1), date(2026, 1, 2))
    t = db.query(EntregaEsperada).one()
    assert t.fecha_vencimiento_nominal.date() == date(2026, 1, 1)
    assert t.fecha_limite_teorica.date() > date(2026, 1, 1), "la gracia amplía"


# --------------------------------------------------------------------------
# Correrlo muchas veces no duplica nada
# --------------------------------------------------------------------------

def test_un_anio_de_corridas_diarias_no_duplica_ni_pierde(db):
    """365 corridas, una por día. Doce obligaciones y ni una repetida.

    El planificador arranca donde acabó la corrida anterior, así que un defecto
    de idempotencia no se ve en la primera corrida ni en la segunda: se ve al
    cabo de un año.
    """
    _informe_con_responsable(db, {
        "modo": "ESPECIFICO", "meses": list(range(1, 13)), "dia_corte": 15,
        "ajuste_habil_anterior": True, "fecha_inicio": "2026-01-01"})

    corridas = _correr_dias(date(2026, 1, 1), date(2026, 12, 31))
    assert corridas == 365

    tareas = db.query(EntregaEsperada).all()
    nominales = [t.fecha_vencimiento_nominal for t in tareas]
    assert len(tareas) == 12, f"{len(tareas)} tareas en vez de 12"
    assert len(set(nominales)) == 12, "hay obligaciones duplicadas"
    assert {n.day for n in nominales} == {15}
    assert all(t.tipo_ajuste == "ANTICIPADO" for t in tareas)


def test_dos_dias_de_la_semana_que_comparten_entrega_NO_se_colapsan(db):
    """Lunes y martes con plazo de gracia: un lunes festivo se entrega el martes.

    Son dos obligaciones que comparten día de entrega. Con la identidad puesta en
    el día de entrega, la segunda se perdía por «ya existe» — y desaparecía una
    obligación real sin dejar rastro.
    """
    _informe_con_responsable(db, {
        "modo": "SEMANAL", "dias_semana": [0, 1], "mover_habil": True,
        "fecha_inicio": "2026-01-01"})

    _correr_dias(date(2026, 1, 1), date(2026, 3, 31))

    tareas = db.query(EntregaEsperada).all()
    nominales = [t.fecha_vencimiento_nominal for t in tareas]
    assert len(set(nominales)) == len(nominales), "una obligación aparece dos veces"

    # Y que el caso ocurre: si ningún festivo hubiera juntado dos entregas, la
    # prueba pasaría sin comprobar nada.
    limites = [t.fecha_limite_teorica for t in tareas]
    assert len(set(limites)) < len(limites), (
        "ningún festivo juntó dos entregas en el periodo; la prueba no mide nada")


def test_el_planificador_no_reescribe_lo_ya_creado(db):
    """Cambiar la regla afecta al futuro, no al pasado.

    `informes` funciona de manera perpetua y las reglas cambian con la norma. Lo
    que ya se generó bajo la regla anterior es historia registrada: si el
    planificador la reescribiera, una entidad perdería la constancia de lo que se
    le exigía entonces.
    """
    inf = _informe_con_responsable(db, {
        "modo": "ESPECIFICO", "meses": list(range(1, 13)), "dia_corte": 5,
        "fecha_inicio": "2026-01-01"})

    _correr_dias(date(2026, 1, 1), date(2026, 3, 31))
    antes = {t.fecha_vencimiento_nominal for t in db.query(EntregaEsperada).all()}
    assert {n.day for n in antes} == {5}

    # La norma cambia: ahora se entrega el 20.
    inf.recurrencia_json = {"modo": "ESPECIFICO", "meses": list(range(1, 13)),
                            "dia_corte": 20, "fecha_inicio": "2026-01-01"}
    db.commit()

    _correr_dias(date(2026, 4, 1), date(2026, 6, 30))
    todas = {t.fecha_vencimiento_nominal for t in db.query(EntregaEsperada).all()}

    assert antes <= todas, "el planificador borró o movió obligaciones pasadas"
    nuevas = {n.day for n in todas - antes}
    assert nuevas == {20}, f"las nuevas deberían caer el 20: {nuevas}"


# --------------------------------------------------------------------------
# Simulación larga: muchos informes, muchos días
# --------------------------------------------------------------------------

def test_simulacion_larga_de_varios_informes_a_la_vez(db):
    """Cuatro reglas distintas conviviendo durante DOS años: 730 corridas.

    Comprueba lo que ninguna prueba de una sola regla puede ver: que el
    planificador no confunde las obligaciones de un informe con las de otro al
    decidir si algo «ya existe».

    Los totales se afirman EXACTOS y con su derivación al lado. Un umbral
    redondo del tipo «más de 100» pasa igual si una de las cuatro reglas deja de
    generar nada, porque las otras tres tapan el hueco.
    """
    reglas = [
        # Día 1 de cada mes con entrega anticipada.
        {"modo": "ESPECIFICO", "meses": list(range(1, 13)), "dia_corte": 1,
         "ajuste_habil_anterior": True, "fecha_inicio": "2026-01-01"},
        # Último día de cada mes con plazo de gracia.
        {"modo": "ESPECIFICO", "meses": list(range(1, 13)), "es_ultimo_dia": True,
         "ajuste_habil": True, "fecha_inicio": "2026-01-01"},
        # Todos los viernes.
        {"modo": "SEMANAL", "dias_semana": [4], "mover_habil_anterior": True,
         "fecha_inicio": "2026-01-01"},
        # Cada 30 días.
        {"modo": "CICLICO", "intervalo": 30, "fecha_inicio": "2026-01-01"},
    ]
    for n, regla in enumerate(reglas):
        _informe_con_responsable(db, regla, nombre=f"Informe {n}",
                                 correo=f"r{n}@x.co")

    desde, hasta = date(2026, 1, 1), date(2027, 12, 31)
    assert _correr_dias(desde, hasta) == 730

    tareas = db.query(EntregaEsperada).all()
    por_informe = {}
    for t in tareas:
        por_informe.setdefault(t.informe_id, []).append(t)

    # Los cuatro generaron algo. Sin esto, un informe mudo pasa inadvertido
    # dentro de un total alto.
    assert len(por_informe) == 4, f"solo {len(por_informe)} informes generaron tareas"

    conteos = sorted(len(v) for v in por_informe.values())

    #: Viernes del rango, contados sin usar el motor. Si el motor y esta cuenta
    #: divergen, uno de los dos está mal y hay que mirar cuál.
    viernes = sum(1 for k in range((hasta - desde).days + 1)
                  if (desde + timedelta(days=k)).weekday() == 4)
    assert viernes == 105

    #: 24 meses + 1. La de más es la obligación de enero de 2028: su entrega se
    #: adelanta a diciembre de 2027, dentro del rango, así que la tarea nace
    #: aquí. Es exactamente la anticipación funcionando, y por eso el número no
    #: es 24.
    #: El de gracia sí son 24: la entrega de diciembre de 2027 se admite hasta
    #: enero de 2028, fuera del rango, pero la obligación nace el 31 de
    #: diciembre y ahí queda contada.
    #: El cíclico: floor(729 / 30) + 1 = 25.
    assert conteos == [24, 25, 25, 105], conteos
    assert len(tareas) == 179

    # Ninguna obligación repetida DENTRO de cada informe.
    for informe_id, lista in por_informe.items():
        nominales = [t.fecha_vencimiento_nominal for t in lista]
        assert len(nominales) == len(set(nominales)), \
            f"informe {informe_id}: obligaciones duplicadas"

    # Ninguna tarea nace después de su propio plazo.
    tardias = [t for t in tareas
               if t.fecha_programada.date() > t.fecha_limite_teorica.date()]
    assert not tardias, f"{len(tardias)} tareas nacieron ya vencidas"

    # Los tres tipos de ajuste quedaron registrados. Si `tipo_ajuste` se
    # guardara siempre igual, no serviría para saber si entregar el día límite
    # era cumplir o llegar tarde.
    assert {t.tipo_ajuste for t in tareas} == {"ANTICIPADO", "GRACIA", "NINGUNO"}
