"""
Automated Alert System
Detects deadline breaches and sends notifications
"""
from datetime import datetime, timedelta
from sqlalchemy import and_
from app.core.database import get_db_session
from app.models.meta_tables import EntregaEsperada, Usuario, Informe
from app.services.email_service import EmailService
from app.core.config import EMAIL_ENABLED, APP_BASE_URL
import asyncio

class AlertService:
    """
    Servicio de alertas automatizado que detecta y notifica:
    - Tareas próximas a vencer (24h antes)
    - Tareas vencidas no entregadas
    - Tareas extemporáneas recién marcadas
    """

    @staticmethod
    async def enviar_alerta_vencimiento(destinatario: str, nombre_usuario: str,
                                        nombre_informe: str, fecha_limite: datetime,
                                        horas_restantes: int):
        """Envía alerta de tarea próxima a vencer"""
        if not EMAIL_ENABLED:
            print(f"⚠️ Would send deadline alert to {destinatario} for {nombre_informe}")
            return False

        asunto = f"⏰ Alerta: Tarea próxima a vencer - {nombre_informe}"

        html = f"""
        <h2>Alerta de Vencimiento</h2>
        <p>Hola {nombre_usuario},</p>
        <p>Le recordamos que tiene una tarea próxima a vencer:</p>
        <div style="background-color: #fff3cd; border: 1px solid #ffc107; border-radius: 5px; padding: 15px; margin: 15px 0;">
            <p><b>Informe:</b> {nombre_informe}</p>
            <p><b>Fecha límite:</b> {fecha_limite.strftime('%d/%m/%Y %H:%M')}</p>
            <p><b>Tiempo restante:</b> {horas_restantes} horas</p>
        </div>
        <p>Por favor, asegúrese de completar la entrega antes del vencimiento.</p>
        <p><a href="{APP_BASE_URL}/mis-pendientes" style="background-color: #2563eb; color: white; padding: 10px 20px; text-decoration: none; border-radius: 5px;">Ver Mis Pendientes</a></p>
        """

        EmailService._enviar_sync(destinatario, asunto, html)
        return True

    @staticmethod
    async def enviar_alerta_incumplimiento(destinatario: str, nombre_usuario: str,
                                           nombre_informe: str, fecha_limite: datetime):
        """Envía alerta de tarea incumplida"""
        if not EMAIL_ENABLED:
            print(f"⚠️ Would send breach alert to {destinatario} for {nombre_informe}")
            return False

        asunto = f"🔴 Alerta: Tarea Incumplida - {nombre_informe}"

        html = f"""
        <h2>Alerta de Incumplimiento</h2>
        <p>Hola {nombre_usuario},</p>
        <p>La siguiente tarea ha superado su fecha límite sin ser entregada:</p>
        <div style="background-color: #f8d7da; border: 1px solid #dc3545; border-radius: 5px; padding: 15px; margin: 15px 0;">
            <p><b>Informe:</b> {nombre_informe}</p>
            <p><b>Fecha límite:</b> {fecha_limite.strftime('%d/%m/%Y %H:%M')}</p>
            <p><b>Estado:</b> INCUMPLIDO</p>
        </div>
        <p>Por favor, complete la entrega lo antes posible.</p>
        <p><a href="{APP_BASE_URL}/mis-pendientes" style="background-color: #dc3545; color: white; padding: 10px 20px; text-decoration: none; border-radius: 5px;">Ver Mis Pendientes</a></p>
        """

        EmailService._enviar_sync(destinatario, asunto, html)
        return True

    @staticmethod
    def _destinatarios(tarea):
        """A quién le llega el aviso de esta entrega.

        Al responsable asignado; y si no hay ninguno, a los supervisores del
        informe. Antes la consulta hacía JOIN contra Usuario, así que una
        entrega sin responsable no alertaba a NADIE: fallaba en silencio, que
        es la peor forma de fallar en un sistema cuyo único trabajo es avisar.
        """
        if tarea.usuario:
            return [tarea.usuario]
        return [u for u in tarea.informe.supervisores if u]

    @staticmethod
    async def procesar_alertas_pendientes():
        """
        Procesa todas las alertas pendientes:
        1. Alerta 24h antes del vencimiento
        2. Alerta de tareas vencidas

        Cada aviso se emite UNA vez por entrega: al emitirlo se sella la fecha
        en la propia entrega. El job corre cada 6 h, y sin ese sello reenviaba
        el mismo correo de incumplimiento cuatro veces al día para siempre,
        porque "vencida y pendiente" no deja de ser cierto nunca.
        """
        from app.services.notification_service import NotificationService

        ahora = datetime.now()
        umbral_24h = ahora + timedelta(hours=24)

        alertas_enviadas = 0
        notifs_pendientes = []  # (uid, tipo, titulo, msg, url, ent_tipo, ent_id)

        with get_db_session() as db:
            # 1. ALERTAS DE PROXIMIDAD (24h antes), solo las no avisadas.
            tareas_proximas = db.query(EntregaEsperada).join(Informe).filter(
                and_(
                    EntregaEsperada.estado == 'PENDIENTE',
                    EntregaEsperada.fecha_limite_teorica <= umbral_24h,
                    EntregaEsperada.fecha_limite_teorica > ahora,
                    EntregaEsperada.alerta_proximidad_en.is_(None),
                )
            ).all()

            for tarea in tareas_proximas:
                horas_restantes = int((tarea.fecha_limite_teorica - ahora).total_seconds() / 3600)
                for usuario in AlertService._destinatarios(tarea):
                    if usuario.email:
                        try:
                            await AlertService.enviar_alerta_vencimiento(
                                usuario.email,
                                usuario.nombre,
                                tarea.informe.nombre,
                                tarea.fecha_limite_teorica,
                                horas_restantes
                            )
                            alertas_enviadas += 1
                        except Exception as e:
                            print(f"❌ Error enviando alerta a {usuario.email}: {e}")
                    notifs_pendientes.append((
                        usuario.id, 'ALERTA',
                        f"Vence pronto: {tarea.informe.nombre}",
                        f"{horas_restantes}h restantes",
                        '/mis-pendientes', 'entrega', tarea.id
                    ))
                # Se sella aunque no hubiera a quién avisar: reintentarlo cada
                # 6 h no va a hacer aparecer un destinatario. La ausencia se
                # ve en la auditoría, no a fuerza de reintentos.
                tarea.alerta_proximidad_en = ahora

            # 2. ALERTAS DE INCUMPLIMIENTO (ya vencidas), solo las no avisadas.
            tareas_vencidas = db.query(EntregaEsperada).join(Informe).filter(
                and_(
                    EntregaEsperada.estado == 'PENDIENTE',
                    EntregaEsperada.fecha_limite_teorica < ahora,
                    EntregaEsperada.alerta_incumplimiento_en.is_(None),
                )
            ).all()

            for tarea in tareas_vencidas:
                for usuario in AlertService._destinatarios(tarea):
                    if usuario.email:
                        try:
                            await AlertService.enviar_alerta_incumplimiento(
                                usuario.email,
                                usuario.nombre,
                                tarea.informe.nombre,
                                tarea.fecha_limite_teorica
                            )
                            alertas_enviadas += 1
                        except Exception as e:
                            print(f"❌ Error enviando alerta a {usuario.email}: {e}")
                    notifs_pendientes.append((
                        usuario.id, 'URGENTE',
                        f"INCUMPLIDA: {tarea.informe.nombre}",
                        "Superó la fecha límite",
                        '/mis-pendientes', 'entrega', tarea.id
                    ))
                tarea.alerta_incumplimiento_en = ahora

            db.commit()

        # Create in-app notifications AFTER the DB session closes
        for uid, tipo, titulo, msg, url, ent_tipo, ent_id in notifs_pendientes:
            NotificationService.crear(uid, tipo, titulo, msg, url, ent_tipo, ent_id)

        print(f"✅ Alertas procesadas: {alertas_enviadas}, notificaciones in-app: {len(notifs_pendientes)}")
        return alertas_enviadas

    @staticmethod
    async def enviar_alerta_supervisor_sla(email_supervisor: str, metricas: dict):
        """Envía reporte de SLA al supervisor"""
        if not EMAIL_ENABLED:
            print(f"⚠️ Would send SLA report to {email_supervisor}")
            return False

        asunto = "📊 Reporte de SLA - Sistema de Gestión"

        html = f"""
        <h2>Reporte de Cumplimiento SLA</h2>
        <p>Resumen del estado actual del sistema:</p>
        <div style="background-color: #e7f3ff; border: 1px solid #2563eb; border-radius: 5px; padding: 15px; margin: 15px 0;">
            <p><b>Total Entregas:</b> {metricas.get('total', 0)}</p>
            <p><b>Cumplidas:</b> {metricas.get('cumplidas', 0)}</p>
            <p><b>Extemporáneas:</b> {metricas.get('extemporaneas', 0)}</p>
            <p><b>Incumplidas:</b> {metricas.get('incumplidas', 0)}</p>
            <p><b>Pendientes:</b> {metricas.get('pendientes', 0)}</p>
            <p style="margin-top: 15px; font-size: 18px;"><b>Tasa de Cumplimiento:</b> {metricas.get('tasa_cumplimiento', 0):.1f}%</p>
        </div>
        <p><a href="{APP_BASE_URL}/analytics" style="background-color: #2563eb; color: white; padding: 10px 20px; text-decoration: none; border-radius: 5px;">Ver Analytics</a></p>
        """

        EmailService._enviar_sync(email_supervisor, asunto, html)
        return True
