import sys
import os
sys.path.append(os.path.dirname(os.path.abspath(__file__)))

from app.core.database import get_db_session
from app.models.meta_tables import MetaVariable, Usuario, MetaRegla
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

        # 2. VARIABLES REUTILIZABLES (Catálogo)
        if db.query(MetaVariable).count() == 0:
            vars_base = [
                MetaVariable(
                    nombre_interno="hora_entrega", 
                    etiqueta_visual="Hora de Entrega (HHMM)", 
                    tipo_dato="HORA_MILITAR",
                    descripcion="Hora en formato militar (0000-2359)"
                ),
                MetaVariable(
                    nombre_interno="monto_gasto", 
                    etiqueta_visual="Monto del Gasto", 
                    tipo_dato="MONEDA",
                    descripcion="Valor numérico positivo sin símbolos"
                ),
                MetaVariable(
                    nombre_interno="observacion", 
                    etiqueta_visual="Observaciones", 
                    tipo_dato="TEXTO_ALFANUM",
                    descripcion="Texto limpio sin caracteres especiales"
                )
            ]
            db.add_all(vars_base)
            db.commit()
            print("✅ Variables creadas.")
            
            # 3. REGLAS ABSTRACTAS
            # Recuperamos la variable para obtener su ID real
            var_hora = db.query(MetaVariable).filter_by(nombre_interno="hora_entrega").first()
            
            if var_hora:
                regla_hora = MetaRegla(
                    nombre_regla="Validar Puntualidad",
                    variable_id=var_hora.id,
                    operador="LESS_THAN_OR_EQUAL", # <=
                    
                    # --- CORRECCIÓN AQUÍ ---
                    valor_comparacion_json=None, # Antes decía valor_referencia_default
                    # -----------------------
                    
                    tipo_consecuencia="NOTIFY_ROLE",
                    payload_json={"mensaje_alerta": "Entrega tardía detectada", "rol_destino": "AUDITOR"}
                )
                db.add(regla_hora)
                db.commit()
                print("✅ Reglas base creadas.")

if __name__ == "__main__":
    inyectar_datos()