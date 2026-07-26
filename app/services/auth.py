from tumbaga import Argon2id, Bcrypt, EsquemaDesconocido, Tumbaga
from app.core.database import get_db_session
from app.models.meta_tables import Usuario, PasswordReset
import logging
import secrets
from datetime import datetime, timedelta

logger = logging.getLogger(__name__)

# La cadena de esquemas. Se ESCRIBE con Argon2id; bcrypt sigue en la lista solo
# para no dejar fuera a los usuarios creados antes de esta migración, y se
# retira cuando `AuthService.avance_migracion()` diga que ya no queda ninguno.
CADENA = Tumbaga([
    Argon2id(),
    Bcrypt(rondas=12),  # solo verifica
])

class AuthService:
    @staticmethod
    def get_password_hash(password: str) -> str:
        return CADENA.cifrar(password)

    @staticmethod
    def verify_password(plain_password: str, hashed_password: str) -> bool:
        """Verifica sin actualizar.

        Para el inicio de sesión se usa `authenticate_user`, que además sube el
        hash de esquema. Este método queda para las comprobaciones sueltas
        (confirmar la contraseña actual antes de cambiarla, por ejemplo), donde
        no hay a quién persistirle nada.
        """
        try:
            return CADENA.verificar(plain_password, hashed_password).valida
        except (EsquemaDesconocido, ValueError):
            return False

    @staticmethod
    def authenticate_user(email: str, password: str):
        """Autentica y, de paso, sube la contraseña al mejor esquema.

        Este es el ÚNICO instante en que la contraseña existe en claro, así que
        es el único momento en que se puede recifrar. Si no se persiste aquí, no
        se migra nunca.
        """
        with get_db_session() as db:
            user = db.query(Usuario).filter(Usuario.email == email).first()
            if not user or not user.activo: return None

            try:
                resultado = CADENA.verificar(password, user.password_hash)
            except (EsquemaDesconocido, ValueError):
                # Hash corrupto o de un esquema retirado: no se deja entrar, pero
                # se registra, porque significa que alguien quedó fuera del sistema.
                logger.error(
                    "El hash del usuario %s no lo reconoce ningún esquema de la cadena", user.id
                )
                return None

            if not resultado.valida: return None

            if resultado.hash_nuevo:
                user.password_hash = resultado.hash_nuevo
                db.commit()
                logger.info(
                    "Contraseña del usuario %s migrada de %s a %s",
                    user.id, resultado.esquema, CADENA.escritor.nombre,
                )
            return user

    @staticmethod
    def avance_migracion():
        """Cuántas contraseñas quedan en el esquema antiguo.

        Es lo que autoriza a retirar `Bcrypt` de la cadena: mientras no esté
        completo, retirarlo deja fuera a quien todavía no ha vuelto a entrar.
        """
        with get_db_session() as db:
            hashes = [h for (h,) in db.query(Usuario.password_hash).all() if h]
            return CADENA.avance(hashes)

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
            user = db.get(Usuario, registro.usuario_id)
            user.password_hash = AuthService.get_password_hash(nueva_pass)
            
            # Quemar token
            registro.usado = True
            db.commit()
            return True, "Contraseña actualizada correctamente"