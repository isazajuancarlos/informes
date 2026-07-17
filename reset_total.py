import sys
import os

# Asegurar que Python encuentre los módulos
sys.path.append(os.path.dirname(os.path.abspath(__file__)))

from app.core.database import Base, engine, get_db_session
# Importamos TODOS los modelos para que SQLAlchemy los reconozca al crear tablas
from app.models.meta_tables import (
    Usuario, Stakeholder, Informe, EntregaEsperada, 
    MetaVariable, MetaRegla, ConfiguracionInformeVariable,
    SolicitudNuevoInforme, PasswordReset, Comentario
)
from app.services.auth import AuthService

def reiniciar_base_de_datos():
    print("------------------------------------------------")
    print("⚠️  INICIANDO REINICIO TOTAL DE LA BASE DE DATOS")
    print("------------------------------------------------")

    # 1. ELIMINAR TABLAS ANTIGUAS
    print("🗑️  Eliminando tablas existentes...")
    try:
        Base.metadata.drop_all(bind=engine)
        print("   -> Tablas eliminadas correctamente.")
    except Exception as e:
        print(f"   -> Error eliminando (puede ser normal si no existían): {e}")

    # 2. CREAR TABLAS NUEVAS
    print("🏗️  Creando nueva estructura (Schema)...")
    Base.metadata.create_all(bind=engine)
    print("   -> Tablas creadas: Usuarios, Informes, Stakeholders, Solicitudes, etc.")

    # 3. SEMBRAR DATOS INICIALES (SEED)
    print("🌱 Sembrando datos por defecto...")
    
    with get_db_session() as db:
        # --- A. CREAR SUPER ADMIN ---
        admin = Usuario(
            nombre="Super Administrador",
            email="admin@co.co",
            password_hash=AuthService.get_password_hash("123456"),
            es_admin=True,
            rol_sistema='ADMIN', # <--- AQUÍ ESTÁ LA COLUMNA QUE FALTABA
            activo=True
        )
        db.add(admin)
        print("   -> Usuario Admin creado: admin@co.co / 123456")

        # --- B. CREAR UN OPERADOR DE PRUEBA ---
        operador = Usuario(
            nombre="Operador Ejemplo",
            email="operador@co.co",
            password_hash=AuthService.get_password_hash("123456"),
            es_admin=False,
            rol_sistema='OPERADOR',
            activo=True
        )
        db.add(operador)
        print("   -> Usuario Operador creado: operador@co.co / 123456")

        # --- C. CREAR UNAS VARIABLES BÁSICAS ---
        vars_base = [
            MetaVariable(nombre_interno="ingresos_totales", etiqueta_visual="Ingresos Totales", tipo_dato="MONEDA"),
            MetaVariable(nombre_interno="numero_radicado", etiqueta_visual="No. Radicado", tipo_dato="TEXTO"),
            MetaVariable(nombre_interno="fecha_corte", etiqueta_visual="Fecha de Corte", tipo_dato="FECHA"),
        ]
        db.add_all(vars_base)
        print("   -> Variables base creadas.")

        db.commit()

    print("------------------------------------------------")
    print("✅ REINICIO COMPLETADO EXITOSAMENTE")
    print("   Ahora puedes ejecutar: python main.py")
    print("------------------------------------------------")

if __name__ == "__main__":
    reiniciar_base_de_datos()