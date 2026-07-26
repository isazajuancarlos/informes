import sys
import os
sys.path.append(os.path.dirname(os.path.abspath(__file__)))

from app.core.database import get_db_session
from app.models.meta_tables import Usuario
from app.services.auth import AuthService

def inyectar_datos():
    print("🌱 Sembrando datos de Alta Calidad...")
    
    # Contraseña por defecto: 123456
    pass_hash = AuthService.get_password_hash("123456")

    with get_db_session() as db:
        # 1. USUARIOS
        if db.query(Usuario).count() == 0:
            db.add(Usuario(nombre="Admin", email="admin@co.co", password_hash=pass_hash, es_admin=True))
            db.add(Usuario(nombre="Operador", email="operador@co.co", password_hash=pass_hash, es_admin=False))
            db.commit()
            print("✅ Usuarios creados (admin@co.co / 123456).")

        # Aquí se sembraban TRES variables de ejemplo —«Hora de Entrega»,
        # «Monto del Gasto», «Observaciones»— con sus reglas. Eran el mismo
        # problema que las nueve columnas fijas: el ejemplo de alguien
        # convertido en el punto de partida de todos.
        #
        # Una instalación nueva arranca con los usuarios y NADA MÁS. La ficha de
        # un informe empieza vacía porque los campos que describen un informe
        # los decide cada entidad, y se crean desde la propia pantalla.
        print("✅ Listo. Los campos de los informes se crean desde el sistema.")


if __name__ == "__main__":
    inyectar_datos()
