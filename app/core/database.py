import os
from sqlalchemy import create_engine, event
from sqlalchemy.orm import sessionmaker, declarative_base
from contextlib import contextmanager
from app.core.config import DATABASE_URL, IS_SQLITE

SQLALCHEMY_DATABASE_URL = DATABASE_URL

if IS_SQLITE:
    # Perfil standalone: SQLite en una sola máquina.
    # - check_same_thread=False: el scheduler y el servidor web viven en hilos
    #   distintos; SQLAlchemy gestiona el acceso.
    # - la carpeta del fichero debe existir antes de abrir la conexión.
    _ruta = SQLALCHEMY_DATABASE_URL.replace("sqlite:///", "")
    if _ruta and _ruta != ":memory:":
        os.makedirs(os.path.dirname(_ruta) or ".", exist_ok=True)
    engine = create_engine(
        SQLALCHEMY_DATABASE_URL,
        connect_args={"check_same_thread": False},
    )

    @event.listens_for(engine, "connect")
    def _sqlite_pragmas(dbapi_conn, _rec):
        # WAL + busy_timeout: concurrencia razonable para una máquina.
        # foreign_keys: SQLite las ignora si no se activan por conexión.
        cur = dbapi_conn.cursor()
        cur.execute("PRAGMA journal_mode=WAL")
        cur.execute("PRAGMA busy_timeout=5000")
        cur.execute("PRAGMA foreign_keys=ON")
        cur.close()
else:
    engine = create_engine(SQLALCHEMY_DATABASE_URL)
SessionLocal = sessionmaker(autocommit=False, autoflush=False, bind=engine)
Base = declarative_base()

# --- NUEVO HELPER PARA LA UI ---
@contextmanager
def get_db_session():
    """Permite usar la BD dentro de las funciones de NiceGUI"""
    db = SessionLocal()
    try:
        yield db
    finally:
        db.close()