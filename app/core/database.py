from sqlalchemy import create_engine
from sqlalchemy.orm import sessionmaker, declarative_base
from contextlib import contextmanager
from app.core.config import DATABASE_URL

SQLALCHEMY_DATABASE_URL = DATABASE_URL

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