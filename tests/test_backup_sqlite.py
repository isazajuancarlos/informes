"""Banco del respaldo de SQLite (perfil `standalone`).

Existe porque `DESPLIEGUE.md` documentaba como pendiente el respaldo automático
en SQLite y describía el apaño: «copiar el volumen». Copiar el archivo de una
base VIVA puede capturar un estado a medias —una transacción sin terminar— y
producir **un respaldo que parece bueno y no restaura**. Es la peor clase de
fallo: se descubre el día que hace falta.

Lo que se prueba no es que se cree un archivo, sino que ese archivo **sea una
base restaurable con los mismos datos**, incluso si hay una escritura abierta
mientras se respalda.
"""
from __future__ import annotations

import os
import sqlite3

import pytest


@pytest.fixture
def servicio(tmp_path, monkeypatch):
    """BackupService apuntando a una base SQLite de usar y tirar."""
    db = tmp_path / "informes.db"
    con = sqlite3.connect(db)
    con.execute("CREATE TABLE entregas (id INTEGER PRIMARY KEY, estado TEXT)")
    con.executemany("INSERT INTO entregas (estado) VALUES (?)",
                    [("CUMPLIDO",), ("PENDIENTE",), ("EXTEMPORANEO",)])
    con.commit()
    con.close()

    from app.services import backup_service as bs
    monkeypatch.setattr(bs, "IS_SQLITE", True)
    monkeypatch.setattr(bs, "DATABASE_URL", f"sqlite:///{db}")
    monkeypatch.setattr(bs.BackupService, "BACKUP_DIR", str(tmp_path / "copias"))
    return bs.BackupService(), db


def _filas(ruta):
    con = sqlite3.connect(ruta)
    try:
        return sorted(r[0] for r in con.execute("SELECT estado FROM entregas"))
    finally:
        con.close()


def test_la_copia_es_una_base_restaurable_con_los_mismos_datos(servicio):
    """No basta con que exista el archivo: tiene que ABRIRSE y tener los datos."""
    svc, db = servicio
    ok, nombre = svc.crear_copia()
    assert ok, nombre
    assert nombre.endswith(".db")

    copia = os.path.join(svc.BACKUP_DIR, nombre)
    assert _filas(copia) == _filas(db)


def test_respalda_aunque_haya_una_transaccion_ABIERTA(servicio):
    """El caso que rompe un `cp`. Con una escritura sin confirmar, la copia debe
    salir CONSISTENTE: con los datos confirmados y sin los que están en vuelo."""
    svc, db = servicio
    otra = sqlite3.connect(db)
    otra.execute("BEGIN")
    otra.execute("INSERT INTO entregas (estado) VALUES ('SIN_CONFIRMAR')")
    try:
        ok, nombre = svc.crear_copia()
        assert ok, nombre
        copia = os.path.join(svc.BACKUP_DIR, nombre)
        filas = _filas(copia)
        assert "SIN_CONFIRMAR" not in filas, "no puede colarse lo no confirmado"
        assert len(filas) == 3
    finally:
        otra.rollback()
        otra.close()


def test_la_copia_no_se_ve_afectada_por_cambios_posteriores(servicio):
    """Una instantánea es una instantánea: si siguiera viva, no serviría de nada."""
    svc, db = servicio
    ok, nombre = svc.crear_copia()
    assert ok

    con = sqlite3.connect(db)
    con.execute("INSERT INTO entregas (estado) VALUES ('POSTERIOR')")
    con.commit()
    con.close()

    assert "POSTERIOR" not in _filas(os.path.join(svc.BACKUP_DIR, nombre))
    assert "POSTERIOR" in _filas(db)


def test_restaurar_devuelve_los_datos_del_respaldo(servicio):
    svc, db = servicio
    ok, nombre = svc.crear_copia()
    assert ok

    con = sqlite3.connect(db)
    con.execute("DELETE FROM entregas")
    con.commit()
    con.close()
    assert _filas(db) == []

    ok, msg = svc.restaurar_copia(nombre)
    assert ok, msg
    assert len(_filas(db)) == 3


def test_restaurar_guarda_una_copia_de_la_base_ANTERIOR(servicio, tmp_path):
    """Restaurar es cuando alguien está nervioso y puede equivocarse de archivo.
    Sin esta red, ese error no tiene vuelta atrás."""
    svc, db = servicio
    ok, nombre = svc.crear_copia()
    assert ok

    con = sqlite3.connect(db)
    con.execute("INSERT INTO entregas (estado) VALUES ('QUE_NO_SE_PIERDA')")
    con.commit()
    con.close()

    svc.restaurar_copia(nombre)

    previas = list(tmp_path.glob("informes.db.previa-*"))
    assert previas, "debe quedar copia de la base anterior"
    assert "QUE_NO_SE_PIERDA" in _filas(previas[0])


def test_si_no_existe_la_base_lo_dice_y_no_deja_archivo(servicio, monkeypatch):
    """Fallar ruidosamente. Un respaldo vacío en la carpeta es peor que ninguno:
    alguien confiará en él el día que lo necesite (directiva 20)."""
    svc, _db = servicio
    from app.services import backup_service as bs
    monkeypatch.setattr(bs, "DATABASE_URL", "sqlite:////ruta/que/no/existe.db")

    ok, msg = svc.crear_copia()
    assert ok is False
    assert "No se encontró" in msg
    assert not any(f.endswith(".db") for f in os.listdir(svc.BACKUP_DIR)) \
        if os.path.isdir(svc.BACKUP_DIR) else True


def test_listar_incluye_las_copias_db_y_no_solo_las_sql(servicio):
    """Antes solo se listaban `*.sql`: en standalone la lista salía vacía y
    parecía que no había respaldos."""
    svc, _db = servicio
    ok, nombre = svc.crear_copia()
    assert ok
    nombres = [c["nombre"] for c in svc.listar_copias()]
    assert nombre in nombres
