"""El flujo es lo único invariante del producto; estas pruebas lo fijan.

Se prueba que el modelo DISCRIMINA, no solo que acepta el camino feliz: una
participación sin parte, o con las dos, tiene que fallar en la base de datos —
no en una validación de Python que alguien pueda saltarse escribiendo SQL.
"""
import os
import sys

import pytest
from sqlalchemy.exc import IntegrityError

os.environ.setdefault("DEPLOYMENT_PROFILE", "standalone")
os.environ["DATABASE_URL"] = "sqlite:///:memory:"
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from app.core.database import Base, engine, SessionLocal  # noqa: E402
from app.models.meta_tables import (  # noqa: E402
    Informe, Participacion, Stakeholder, Usuario,
    ROL_ORIGEN, ROL_DESTINO, ROL_USUARIO, ROL_EJECUTA, ROL_SUPERVISA,
)


@pytest.fixture
def db():
    Base.metadata.create_all(engine)
    s = SessionLocal()
    yield s
    s.close()
    Base.metadata.drop_all(engine)


def _informe(db, nombre):
    i = Informe(nombre=nombre)
    db.add(i)
    db.flush()
    return i


def _usuario(db, nombre, email):
    u = Usuario(nombre=nombre, email=email, password_hash="x")
    db.add(u)
    db.flush()
    return u


def _tercero(db, nombre):
    s = Stakeholder(nombre=nombre, tipo="EXTERNO")
    db.add(s)
    db.flush()
    return s


# --- El CHECK: una parte es interna O externa, nunca las dos ni ninguna ------

def test_participacion_sin_parte_es_rechazada(db):
    db.add(Participacion(informe=_informe(db, "A"), rol=ROL_ORIGEN))
    with pytest.raises(IntegrityError):
        db.commit()


def test_participacion_con_ambas_partes_es_rechazada(db):
    db.add(Participacion(
        informe=_informe(db, "B"),
        usuario=_usuario(db, "Ana", "ana@x.co"),
        stakeholder=_tercero(db, "ARL"),
        rol=ROL_ORIGEN,
    ))
    with pytest.raises(IntegrityError):
        db.commit()


def test_no_se_duplica_la_misma_parte_en_el_mismo_rol(db):
    inf, u = _informe(db, "C"), _usuario(db, "Ana", "ana2@x.co")
    db.add(Participacion(informe=inf, usuario=u, rol=ROL_EJECUTA))
    db.commit()
    db.add(Participacion(informe=inf, usuario=u, rol=ROL_EJECUTA))
    with pytest.raises(IntegrityError):
        db.commit()


def test_la_misma_parte_puede_tener_dos_roles_distintos(db):
    inf, u = _informe(db, "D"), _usuario(db, "Ana", "ana3@x.co")
    db.add(Participacion(informe=inf, usuario=u, rol=ROL_EJECUTA))
    db.add(Participacion(informe=inf, usuario=u, rol=ROL_SUPERVISA))
    db.commit()
    assert len(inf.participaciones) == 2


# --- tipo_flujo se DERIVA: no puede contradecir a las partes ----------------

def test_flujo_interno_cuando_todas_las_partes_son_internas(db):
    inf = _informe(db, "E")
    db.add(Participacion(informe=inf, usuario=_usuario(db, "A", "a@x.co"), rol=ROL_ORIGEN))
    db.add(Participacion(informe=inf, usuario=_usuario(db, "B", "b@x.co"), rol=ROL_DESTINO))
    db.commit()
    assert inf.tipo_flujo == "ENVIO_INTERNO"


def test_envio_externo_cuando_el_destino_es_un_tercero(db):
    inf = _informe(db, "F")
    db.add(Participacion(informe=inf, usuario=_usuario(db, "A", "a2@x.co"), rol=ROL_ORIGEN))
    db.add(Participacion(informe=inf, stakeholder=_tercero(db, "ARL"), rol=ROL_DESTINO))
    db.commit()
    assert inf.tipo_flujo == "ENVIO_EXTERNO"


def test_recepcion_externa_cuando_el_origen_es_un_tercero(db):
    inf = _informe(db, "G")
    db.add(Participacion(informe=inf, stakeholder=_tercero(db, "Laboratorio"), rol=ROL_ORIGEN))
    db.add(Participacion(informe=inf, usuario=_usuario(db, "A", "a3@x.co"), rol=ROL_DESTINO))
    db.commit()
    assert inf.tipo_flujo == "RECEPCION_EXTERNA"


def test_bidireccional_cuando_entra_y_sale(db):
    inf = _informe(db, "H")
    db.add(Participacion(informe=inf, stakeholder=_tercero(db, "Lab"), rol=ROL_ORIGEN))
    db.add(Participacion(informe=inf, stakeholder=_tercero(db, "Cliente"), rol=ROL_DESTINO))
    db.commit()
    assert inf.tipo_flujo == "BIDIRECCIONAL"


def test_el_flujo_cambia_solo_al_cambiar_las_partes(db):
    """La regresión que motivó todo esto: antes `tipo_flujo` era una columna
    que se quedaba en ENVIO_EXTERNO aunque le quitaras el destinatario externo."""
    inf = _informe(db, "I")
    ext = Participacion(informe=inf, stakeholder=_tercero(db, "Banco"), rol=ROL_DESTINO)
    db.add(ext)
    db.commit()
    assert inf.tipo_flujo == "ENVIO_EXTERNO"

    db.delete(ext)
    db.commit()
    db.refresh(inf)
    assert inf.tipo_flujo == "ENVIO_INTERNO"


# --- Las partes internas y externas se consultan igual ----------------------

def test_partes_filtra_por_rol_y_por_naturaleza(db):
    inf = _informe(db, "J")
    db.add(Participacion(informe=inf, usuario=_usuario(db, "Int", "i@x.co"),
                         rol=ROL_USUARIO, es_principal=True, orden=0))
    db.add(Participacion(informe=inf, stakeholder=_tercero(db, "Ext"),
                         rol=ROL_USUARIO, es_principal=False, orden=1))
    db.commit()

    assert len(inf.partes(ROL_USUARIO)) == 2
    assert len(inf.partes(ROL_USUARIO, externas=True)) == 1
    assert len(inf.partes(ROL_USUARIO, externas=False)) == 1
    # El principal va primero, sin importar el orden de inserción.
    assert inf.partes(ROL_USUARIO)[0].nombre == "Int"


def test_suplentes_van_despues_del_principal(db):
    inf = _informe(db, "K")
    db.add(Participacion(informe=inf, usuario=_usuario(db, "Suplente", "s@x.co"),
                         rol=ROL_EJECUTA, es_principal=False, orden=0))
    db.add(Participacion(informe=inf, usuario=_usuario(db, "Titular", "t@x.co"),
                         rol=ROL_EJECUTA, es_principal=True, orden=1))
    db.commit()
    assert [p.nombre for p in inf.partes(ROL_EJECUTA)] == ["Titular", "Suplente"]
