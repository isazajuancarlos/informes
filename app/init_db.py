from app.core.database import engine, Base
from app.models.meta_tables import MetaVariable, MetaRegla, Usuario, Informe, Participacion

def init_db():
    """
    MODO SEGURO: Solo crea tablas si faltan.
    """
    try:
        print("🛡️  Verificando esquema de base de datos...")
        Base.metadata.create_all(bind=engine)
        print("✅ Base de datos lista.")
    except Exception as e:
        print(f"❌ Error al conectar con la BD: {e}")

if __name__ == "__main__":
    init_db()