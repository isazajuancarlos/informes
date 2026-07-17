"""
Session Management and Security
Handles session timeout, activity tracking, and security features
"""
from datetime import datetime, timedelta
from nicegui import app, ui
from app.core.config import SESSION_TIMEOUT_MINUTES
from functools import wraps

class SessionManager:
    """Gestiona sesiones de usuario con timeout y rastreo de actividad"""

    @staticmethod
    def init_session():
        """Inicializa timestamp de sesión"""
        app.storage.user['last_activity'] = datetime.now().isoformat()
        app.storage.user['session_started'] = datetime.now().isoformat()

    @staticmethod
    def update_activity():
        """Actualiza timestamp de última actividad"""
        app.storage.user['last_activity'] = datetime.now().isoformat()

    @staticmethod
    def is_session_expired():
        """Verifica si la sesión ha expirado"""
        if not app.storage.user.get('authenticated', False):
            return False  # No autenticado, no hay sesión que expirar

        last_activity_str = app.storage.user.get('last_activity')
        if not last_activity_str:
            return True  # Sin actividad registrada = expirada

        try:
            last_activity = datetime.fromisoformat(last_activity_str)
            timeout_delta = timedelta(minutes=SESSION_TIMEOUT_MINUTES)

            if datetime.now() - last_activity > timeout_delta:
                return True
        except (ValueError, TypeError):
            return True

        return False

    @staticmethod
    def get_session_info():
        """Obtiene información de la sesión actual"""
        if not app.storage.user.get('authenticated', False):
            return None

        last_activity_str = app.storage.user.get('last_activity')
        session_started_str = app.storage.user.get('session_started')

        info = {
            'user_email': app.storage.user.get('email'),
            'user_name': app.storage.user.get('nombre'),
            'role': app.storage.user.get('rol'),
            'is_admin': app.storage.user.get('es_admin', False),
            'last_activity': last_activity_str,
            'session_started': session_started_str,
            'timeout_minutes': SESSION_TIMEOUT_MINUTES
        }

        if last_activity_str:
            try:
                last_activity = datetime.fromisoformat(last_activity_str)
                expires_at = last_activity + timedelta(minutes=SESSION_TIMEOUT_MINUTES)
                info['expires_at'] = expires_at.isoformat()
                info['time_remaining_minutes'] = int((expires_at - datetime.now()).total_seconds() / 60)
            except (ValueError, TypeError):
                pass

        return info

    @staticmethod
    def invalidate_session():
        """Invalida la sesión actual"""
        app.storage.user.clear()

def require_auth_with_timeout():
    """
    Decorator para verificar autenticación y timeout de sesión
    Uso: @require_auth_with_timeout()
    """
    def decorator(func):
        @wraps(func)
        def wrapper(*args, **kwargs):
            # Verificar si está autenticado
            if not app.storage.user.get('authenticated', False):
                ui.navigate.to('/login')
                return None

            # Verificar timeout de sesión
            if SessionManager.is_session_expired():
                ui.notify('Sesión expirada. Por favor, inicia sesión nuevamente.', type='warning')
                SessionManager.invalidate_session()
                ui.navigate.to('/login')
                return None

            # Actualizar actividad
            SessionManager.update_activity()

            # Ejecutar función
            return func(*args, **kwargs)

        return wrapper
    return decorator

def require_admin_with_timeout():
    """
    Decorator para verificar autenticación de admin y timeout
    Uso: @require_admin_with_timeout()
    """
    def decorator(func):
        @wraps(func)
        def wrapper(*args, **kwargs):
            # Verificar autenticación primero
            if not app.storage.user.get('authenticated', False):
                ui.navigate.to('/login')
                return None

            # Verificar timeout
            if SessionManager.is_session_expired():
                ui.notify('Sesión expirada. Por favor, inicia sesión nuevamente.', type='warning')
                SessionManager.invalidate_session()
                ui.navigate.to('/login')
                return None

            # Verificar rol admin
            if not app.storage.user.get('es_admin', False):
                ui.notify('Acceso Restringido', type='negative')
                ui.navigate.to('/')
                return None

            # Actualizar actividad
            SessionManager.update_activity()

            # Ejecutar función
            return func(*args, **kwargs)

        return wrapper
    return decorator


def _persist_audit_event(evento: str, email: str = None, exitoso: bool = None,
                          ip_address: str = None, recurso: str = None, detalles: dict = None):
    """
    Persiste un evento de auditoría en la base de datos.
    Implementa trazabilidad requerida por Ley 1581/2012 (Habeas Data).
    Falla silenciosamente para no bloquear el flujo de autenticación.
    """
    try:
        from app.core.database import get_db_session
        from app.models.meta_tables import AuditLog
        with get_db_session() as db:
            log = AuditLog(
                evento=evento,
                email=email,
                exitoso=exitoso,
                ip_address=ip_address,
                recurso=recurso,
                detalles=detalles,
            )
            db.add(log)
            db.commit()
    except Exception as exc:
        # No bloquear el flujo de autenticación por falla de auditoría
        print(f"⚠️ Error al persistir audit log: {exc}")


class SecurityAudit:
    """Rastreo de eventos de seguridad — Ley 1581/2012 / ISO 27001 A.12.4"""

    @staticmethod
    def log_login_attempt(email: str, success: bool, ip_address: str = None):
        """Registra intento de login en consola y en BD."""
        print(f"🔐 login_attempt email={email} success={success} ip={ip_address}")
        _persist_audit_event(
            evento='login_attempt',
            email=email,
            exitoso=success,
            ip_address=ip_address,
        )

    @staticmethod
    def log_logout(email: str):
        """Registra logout en consola y en BD."""
        print(f"🚪 logout email={email}")
        _persist_audit_event(evento='logout', email=email, exitoso=True)

    @staticmethod
    def log_session_timeout(email: str):
        """Registra timeout de sesión en consola y en BD."""
        print(f"⏱️ session_timeout email={email}")
        _persist_audit_event(evento='session_timeout', email=email)

    @staticmethod
    def log_permission_denied(email: str, resource: str):
        """Registra acceso denegado en consola y en BD."""
        print(f"⛔ permission_denied email={email} resource={resource}")
        _persist_audit_event(
            evento='permission_denied',
            email=email,
            exitoso=False,
            recurso=resource,
        )
