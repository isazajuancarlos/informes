"""
In-app notification service — creates and manages NotificacionApp records.
"""
from app.core.database import get_db_session
from app.core.tiempo import ahora_utc
from app.models.meta_tables import NotificacionApp


class NotificationService:

    @staticmethod
    def crear(usuario_id, tipo, titulo, mensaje, url_destino=None, entidad_tipo=None, entidad_id=None):
        """Create an in-app notification for a user."""
        try:
            with get_db_session() as db:
                db.add(NotificacionApp(
                    usuario_id=usuario_id,
                    tipo=tipo,
                    titulo=titulo,
                    mensaje=mensaje,
                    leida=False,
                    fecha_creacion=ahora_utc(),
                    url_destino=url_destino,
                    entidad_tipo=entidad_tipo,
                    entidad_id=entidad_id
                ))
                db.commit()
        except Exception as e:
            print(f"[NotificationService] Error creating notification: {e}")

    @staticmethod
    def marcar_leida(notificacion_id, usuario_id):
        """Mark a single notification as read."""
        try:
            with get_db_session() as db:
                n = db.query(NotificacionApp).filter_by(
                    id=notificacion_id, usuario_id=usuario_id
                ).first()
                if n:
                    n.leida = True
                    db.commit()
        except Exception as e:
            print(f"[NotificationService] Error marking read: {e}")

    @staticmethod
    def marcar_todas_leidas(usuario_id):
        """Mark all unread notifications for a user as read."""
        try:
            with get_db_session() as db:
                db.query(NotificacionApp).filter_by(
                    usuario_id=usuario_id, leida=False
                ).update({"leida": True})
                db.commit()
        except Exception as e:
            print(f"[NotificationService] Error marking all read: {e}")

    @staticmethod
    def obtener_no_leidas(usuario_id) -> list:
        """Return unread notifications for a user (detached from session)."""
        try:
            with get_db_session() as db:
                items = db.query(NotificacionApp).filter_by(
                    usuario_id=usuario_id, leida=False
                ).order_by(NotificacionApp.fecha_creacion.desc()).all()
                db.expunge_all()
                return items
        except Exception as e:
            print(f"[NotificationService] Error fetching unread: {e}")
            return []

    @staticmethod
    def obtener_recientes(usuario_id, limit=50) -> list:
        """Return recent notifications for a user (detached from session)."""
        try:
            with get_db_session() as db:
                items = db.query(NotificacionApp).filter_by(
                    usuario_id=usuario_id
                ).order_by(NotificacionApp.fecha_creacion.desc()).limit(limit).all()
                db.expunge_all()
                return items
        except Exception as e:
            print(f"[NotificationService] Error fetching recent: {e}")
            return []
