import bcrypt
from app.core.database import get_db_session
from app.models.meta_tables import Usuario, PasswordReset
import secrets
from datetime import datetime, timedelta

# Work factor para bcrypt (12 es recomendado para 2024+)
BCRYPT_ROUNDS = 12

class AuthService:
    @staticmethod
    def get_password_hash(password: str) -> str:
        password_bytes = password.encode('utf-8')
        salt = bcrypt.gensalt(rounds=BCRYPT_ROUNDS)
        hashed = bcrypt.hashpw(password_bytes, salt)
        return hashed.decode('utf-8')

    @staticmethod
    def verify_password(plain_password: str, hashed_password: str) -> bool:
        try:
            password_bytes = plain_password.encode('utf-8')
            hashed_bytes = hashed_password.encode('utf-8')
            return bcrypt.checkpw(password_bytes, hashed_bytes)
        except Exception:
            return False

    @staticmethod
    def authenticate_user(email: str, password: str):
        with get_db_session() as db:
            user = db.query(Usuario).filter(Usuario.email == email).first()
            if not user or not user.activo: return None
            if not AuthService.verify_password(password, user.password_hash): return None
            return user

    @staticmethod
    def generar_token_recuperacion(email: str):
        """Crea un token válido por 1 hora"""
        with get_db_session() as db:
            user = db.query(Usuario).filter(Usuario.email == email).first()
            if not user: return None
            
            # Generar token seguro
            token = secrets.token_urlsafe(32)
            expira = datetime.now() + timedelta(hours=1)
            
            reset_entry = PasswordReset(usuario_id=user.id, token=token, fecha_expiracion=expira)
            db.add(reset_entry)
            db.commit()
            return token

    @staticmethod
    def consumir_token_y_cambiar_pass(token: str, nueva_pass: str):
        """Valida el token y cambia la contraseña"""
        with get_db_session() as db:
            registro = db.query(PasswordReset).filter(
                PasswordReset.token == token,
                PasswordReset.usado == False,
                PasswordReset.fecha_expiracion > datetime.now()
            ).first()
            
            if not registro: return False, "Token inválido o expirado"
            
            # Cambiar password
            user = db.query(Usuario).get(registro.usuario_id)
            user.password_hash = AuthService.get_password_hash(nueva_pass)
            
            # Quemar token
            registro.usado = True
            db.commit()
            return True, "Contraseña actualizada correctamente"