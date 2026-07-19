"""El único trabajo del sistema es avisar. Estas pruebas fijan cuándo NO avisa.

Los dos fallos que motivaron el arreglo:
  1. El aviso de incumplimiento se reenviaba en cada corrida del job (cada 6 h)
     porque su condición —vencida y pendiente— no deja de cumplirse nunca.
  2. Una entrega sin responsable no alertaba a nadie, porque la consulta hacía
     JOIN contra Usuario. Fallaba en silencio.
"""
import asyncio
import os
import sys
from datetime import datetime, timedelta

import pytest

os.environ.setdefault("DEPLOYMENT_PROFILE", "standalone")
os.environ["DATABASE_URL"] = "sqlite:///:memory:"
os.environ["EMAIL_ENABLED"] = "false"
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from app.core.database import Base, SessionLocal, engine  # noqa: E402
from app.models.meta_tables import (  # noqa: E402
    EntregaEsperada, Informe, NotificacionApp, Participacion, Usuario,
    ROL_EJECUTA, ROL_SUPERVISA,
)
from app.services.alert_service import AlertService  # noqa: E402


@pytest.fixture
def db():
    Base.metadata.create_all(engine)
    s = SessionLocal()
    yield s
    s.close()
    Base.metadata.drop_all(engine)


def _montar(db, con_responsable=True):
    """Un informe vencido, con supervisor, con o sin responsable asignado."""
    ejecutor = Usuario(nombre="Lucía", email="lucia@x.co", password_hash="h")
    supervisor = Usuario(nombre="Camilo", email="camilo@x.co", password_hash="h")
    db.add_all([ejecutor, supervisor])
    inf = Informe(nombre="Reporte vencido")
    db.add(inf)
    db.flush()
    db.add(Participacion(informe_id=inf.id, usuario_id=ejecutor.id, rol=ROL_EJECUTA))
    db.add(Participacion(informe_id=inf.id, usuario_id=supervisor.id, rol=ROL_SUPERVISA))
    ent = EntregaEsperada(
        informe_id=inf.id,
        usuario_id=ejecutor.id if con_responsable else None,
        fecha_limite_teorica=datetime.now() - timedelta(days=2),
        estado="PENDIENTE",
    )
    db.add(ent)
    db.commit()
    return inf, ent, ejecutor, supervisor


def _correr():
    return asyncio.run(AlertService.procesar_alertas_pendientes())


def _notis(db):
    return db.query(NotificacionApp).count()


# --- 1. No se repite -------------------------------------------------------

def test_el_aviso_de_incumplimiento_se_emite_una_sola_vez(db):
    _montar(db)

    _correr()
    tras_primera = _notis(db)
    assert tras_primera == 1, "la primera corrida debe avisar"

    # El job vuelve a correr tres veces (18 h de operación real).
    for _ in range(3):
        _correr()

    assert _notis(db) == tras_primera, (
        "el aviso se repitió: esto genera 4 correos diarios por entrega "
        "incumplida, indefinidamente"
    )


def test_la_entrega_queda_sellada_con_la_fecha_del_aviso(db):
    _, ent, _, _ = _montar(db)
    assert ent.alerta_incumplimiento_en is None

    _correr()
    db.expire_all()
    ent = db.query(EntregaEsperada).first()
    assert ent.alerta_incumplimiento_en is not None


# --- 2. No falla en silencio ----------------------------------------------

def test_sin_responsable_el_aviso_va_al_supervisor(db):
    _, _, _, supervisor = _montar(db, con_responsable=False)

    _correr()

    notis = db.query(NotificacionApp).all()
    assert len(notis) == 1, "una entrega sin responsable no puede quedar sin aviso"
    assert notis[0].usuario_id == supervisor.id


def test_con_responsable_el_aviso_no_va_al_supervisor(db):
    """El supervisor es el destinatario de último recurso, no un copiado
    permanente: inundarlo lo vuelve sordo."""
    _, _, ejecutor, supervisor = _montar(db, con_responsable=True)

    _correr()

    destinatarios = [n.usuario_id for n in db.query(NotificacionApp).all()]
    assert destinatarios == [ejecutor.id]
    assert supervisor.id not in destinatarios


def test_sin_responsable_ni_supervisor_no_revienta(db):
    """Nadie a quien avisar es un dato para la auditoría, no una excepción."""
    inf = Informe(nombre="Huérfano")
    db.add(inf)
    db.flush()
    db.add(EntregaEsperada(informe_id=inf.id, usuario_id=None,
                           fecha_limite_teorica=datetime.now() - timedelta(days=1),
                           estado="PENDIENTE"))
    db.commit()

    _correr()  # no debe lanzar

    db.expire_all()
    ent = db.query(EntregaEsperada).first()
    assert ent.alerta_incumplimiento_en is not None, (
        "debe sellarse igual: reintentar cada 6 h no hará aparecer un destinatario"
    )
    assert _notis(db) == 0


# --- 3. Sigue avisando de lo que toca -------------------------------------

def test_avisa_de_lo_que_esta_por_vencer(db):
    ejecutor = Usuario(nombre="Ana", email="ana@x.co", password_hash="h")
    db.add(ejecutor)
    inf = Informe(nombre="Vence mañana")
    db.add(inf)
    db.flush()
    db.add(EntregaEsperada(informe_id=inf.id, usuario_id=ejecutor.id,
                           fecha_limite_teorica=datetime.now() + timedelta(hours=10),
                           estado="PENDIENTE"))
    db.commit()

    _correr()

    notis = db.query(NotificacionApp).all()
    assert len(notis) == 1
    assert notis[0].tipo == "ALERTA"


def test_no_avisa_de_lo_ya_entregado(db):
    _, ent, _, _ = _montar(db)
    ent.estado = "CUMPLIDO"
    db.commit()

    _correr()

    assert _notis(db) == 0
