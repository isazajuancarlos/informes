import smtplib
from email.mime.text import MIMEText
from email.mime.multipart import MIMEMultipart
from nicegui import run
import os
from app.core.config import SMTP_HOST, SMTP_PORT, SENDER_EMAIL, SENDER_PASSWORD, EMAIL_ENABLED, APP_BASE_URL

class EmailService:
    # --- CONFIGURACIÓN desde variables de entorno ---
    SMTP_SERVER = SMTP_HOST
    SMTP_PORT = SMTP_PORT
    SENDER_EMAIL = SENDER_EMAIL
    SENDER_PASSWORD = SENDER_PASSWORD
    DISPLAY_NAME = "Sistema de Gestión"

    # URL BASE DEL SISTEMA — configurable via APP_BASE_URL en .env
    BASE_URL = APP_BASE_URL

    @staticmethod
    async def enviar_credenciales(destinatario, nombre, password):
        """Correo de Bienvenida"""
        if not EMAIL_ENABLED:
            print(f"⚠️ Email not configured. Would send credentials to {destinatario}")
            return False

        html = f"""
        <h2>Bienvenido al Sistema</h2>
        <p>Hola {nombre}, sus credenciales son:</p>
        <p><b>Usuario:</b> {destinatario}<br><b>Contraseña:</b> {password}</p>
        """
        await run.io_bound(EmailService._enviar_sync, destinatario, "Credenciales de Acceso", html)

    @staticmethod
    async def enviar_recuperacion(destinatario, token):
        """Correo de Restauración de Contraseña"""
        if not EMAIL_ENABLED:
            print(f"⚠️ Email not configured. Would send recovery token to {destinatario}")
            return False

        link = f"{EmailService.BASE_URL}/reset-password?token={token}"

        html = f"""
        <h2>Recuperación de Contraseña</h2>
        <p>Hemos recibido una solicitud para restablecer su contraseña.</p>
        <p>Haga clic en el siguiente enlace para continuar (válido por 1 hora):</p>
        <p><a href="{link}" style="background-color: #2563eb; color: white; padding: 10px 20px; text-decoration: none; border-radius: 5px;">Restablecer Contraseña</a></p>
        <p>Si usted no solicitó esto, ignore este mensaje.</p>
        """
        await run.io_bound(EmailService._enviar_sync, destinatario, "Restablecer Contraseña", html)

    @staticmethod
    async def enviar_notificacion_solicitud(destinatario, estado, respuesta):
        """Notifica al stakeholder sobre aprobación/rechazo de su solicitud."""
        if not EMAIL_ENABLED:
            print(f"⚠️ Email not configured. Would send request status to {destinatario}")
            return False
        color = '#22c55e' if estado == 'APROBADA' else '#ef4444'
        asunto = f"Solicitud {estado}"
        html = f"""<h2>Respuesta a su solicitud</h2>
        <div style="border-left:4px solid {color};padding:10px;margin:15px 0">
            <b>Estado:</b> {estado}<br><b>Respuesta:</b> {respuesta}
        </div>
        <a href="{EmailService.BASE_URL}/portal-terceros" style="background-color:#2563eb;color:white;padding:10px 20px;text-decoration:none;border-radius:5px;">Ver Portal</a>"""
        await run.io_bound(EmailService._enviar_sync, destinatario, asunto, html)

    @staticmethod
    async def enviar_asignacion_manual(destinatario, nombre_informe, fecha_limite, notas=None):
        """Notifica al operador sobre una tarea asignada manualmente."""
        if not EMAIL_ENABLED:
            print(f"⚠️ Email not configured. Would send manual assignment to {destinatario}")
            return False
        asunto = f"Nueva tarea asignada: {nombre_informe}"
        notas_html = f"<p><b>Notas:</b> {notas}</p>" if notas else ""
        html = f"""<h2>Se te ha asignado una tarea</h2>
        <p><b>Informe:</b> {nombre_informe}</p>
        <p><b>Fecha límite:</b> {fecha_limite}</p>{notas_html}
        <a href="{EmailService.BASE_URL}/mis-pendientes" style="background-color:#2563eb;color:white;padding:10px 20px;text-decoration:none;border-radius:5px;">Ver Mis Pendientes</a>"""
        await run.io_bound(EmailService._enviar_sync, destinatario, asunto, html)

    @staticmethod
    def _enviar_sync(destinatario, asunto, cuerpo_html):
        try:
            msg = MIMEMultipart()
            msg['From'] = f"{EmailService.DISPLAY_NAME} <{EmailService.SENDER_EMAIL}>"
            msg['To'] = destinatario
            msg['Subject'] = asunto
            msg.attach(MIMEText(cuerpo_html, 'html'))

            server = smtplib.SMTP(EmailService.SMTP_SERVER, EmailService.SMTP_PORT)
            server.starttls()
            server.login(EmailService.SENDER_EMAIL, EmailService.SENDER_PASSWORD)
            server.sendmail(EmailService.SENDER_EMAIL, destinatario, msg.as_string())
            server.quit()
            print(f"✅ Correo enviado a {destinatario}")
            return True
        except Exception as e:
            print(f"❌ Error enviando correo: {e}")
            return False