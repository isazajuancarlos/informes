"""
Stakeholder Requests Management
Admin interface to approve/reject report requests from stakeholders
"""
from nicegui import ui, app
from app.core.database import get_db_session
from app.models.meta_tables import SolicitudNuevoInforme, Stakeholder, Usuario
from datetime import datetime
import asyncio
from app.core.tiempo import formatear_utc

def page_requests_manager():
    """Gestión de Solicitudes de Terceros"""

    ui.label('Gestión de Solicitudes').classes('text-3xl font-bold text-slate-800 mb-4')
    ui.label('Revisa y responde solicitudes de nuevos informes de terceros').classes('text-slate-500 mb-6')

    # Estado para refrescar
    requests_container = ui.column().classes('w-full gap-4')

    def cargar_solicitudes():
        requests_container.clear()

        with get_db_session() as db:
            solicitudes = db.query(SolicitudNuevoInforme).join(Stakeholder).order_by(
                SolicitudNuevoInforme.fecha_solicitud.desc()
            ).all()

            if not solicitudes:
                with requests_container:
                    with ui.card().classes('w-full p-8 text-center'):
                        ui.icon('inbox', size='xl').classes('text-slate-300 mb-2')
                        ui.label('No hay solicitudes pendientes').classes('text-slate-400')
                return

            for sol in solicitudes:
                with requests_container:
                    crear_tarjeta_solicitud(sol, cargar_solicitudes, db)

    def crear_tarjeta_solicitud(solicitud, refresh_callback, db):
        """Crea tarjeta individual de solicitud"""

        # Color según estado
        color_map = {
            'PENDIENTE': 'border-orange-400 bg-orange-50',
            'APROBADA': 'border-green-400 bg-green-50',
            'RECHAZADA': 'border-red-400 bg-red-50'
        }
        card_color = color_map.get(solicitud.estado, 'border-gray-300')

        with ui.card().classes(f'w-full border-l-4 {card_color}'):
            # Header
            with ui.row().classes('w-full items-start justify-between'):
                with ui.column().classes('gap-1 flex-grow'):
                    ui.label(solicitud.titulo).classes('text-lg font-bold text-slate-800')
                    ui.label(f'Solicitante: {solicitud.stakeholder.nombre}').classes('text-sm text-slate-600')
                    ui.label(f'Fecha: {formatear_utc(solicitud.fecha_solicitud, "%d/%m/%Y %H:%M")}').classes('text-xs text-slate-400')

                # Badge de estado
                badge_color = {
                    'PENDIENTE': 'bg-orange-500',
                    'APROBADA': 'bg-green-500',
                    'RECHAZADA': 'bg-red-500'
                }
                ui.badge(solicitud.estado, color='white').classes(
                    f'{badge_color.get(solicitud.estado, "bg-gray-500")} text-white'
                )

            ui.separator()

            # Descripción
            with ui.column().classes('gap-2'):
                ui.label('Descripción:').classes('text-xs font-bold text-slate-500 uppercase')
                ui.label(solicitud.descripcion or '(Sin descripción)').classes('text-sm text-slate-700 whitespace-pre-wrap')

            # Respuesta del admin (si existe)
            if solicitud.respuesta_admin:
                ui.separator()
                with ui.column().classes('gap-2 bg-blue-50 p-3 rounded'):
                    ui.label('Respuesta del Administrador:').classes('text-xs font-bold text-blue-700 uppercase')
                    ui.label(solicitud.respuesta_admin).classes('text-sm text-slate-700 whitespace-pre-wrap')

            # Acciones (solo si está pendiente)
            if solicitud.estado == 'PENDIENTE':
                ui.separator()

                with ui.row().classes('w-full gap-2 justify-end'):
                    # Campo de respuesta
                    respuesta_input = ui.textarea(
                        label='Respuesta',
                        placeholder='Escribe una respuesta al solicitante...'
                    ).classes('flex-grow').props('outlined rows=2')

                    def aprobar():
                        from app.services.email_service import EmailService
                        from app.services.notification_service import NotificationService
                        respuesta = respuesta_input.value
                        if not respuesta:
                            ui.notify('Debes escribir una respuesta', type='warning')
                            return

                        solicitud.estado = 'APROBADA'
                        solicitud.respuesta_admin = respuesta
                        stakeholder_email = solicitud.stakeholder.email
                        stakeholder_uid = solicitud.stakeholder.usuario_id
                        db.commit()
                        ui.notify('Solicitud aprobada', type='positive')

                        if stakeholder_email:
                            asyncio.create_task(EmailService.enviar_notificacion_solicitud(
                                stakeholder_email, 'APROBADA', respuesta))
                        if stakeholder_uid:
                            NotificationService.crear(
                                stakeholder_uid, 'INFO',
                                'Solicitud aprobada',
                                respuesta[:200],
                                '/portal-terceros'
                            )
                        refresh_callback()

                    def rechazar():
                        from app.services.email_service import EmailService
                        from app.services.notification_service import NotificationService
                        respuesta = respuesta_input.value
                        if not respuesta:
                            ui.notify('Debes escribir una respuesta explicando el rechazo', type='warning')
                            return

                        solicitud.estado = 'RECHAZADA'
                        solicitud.respuesta_admin = respuesta
                        stakeholder_email = solicitud.stakeholder.email
                        stakeholder_uid = solicitud.stakeholder.usuario_id
                        db.commit()
                        ui.notify('Solicitud rechazada', type='negative')

                        if stakeholder_email:
                            asyncio.create_task(EmailService.enviar_notificacion_solicitud(
                                stakeholder_email, 'RECHAZADA', respuesta))
                        if stakeholder_uid:
                            NotificationService.crear(
                                stakeholder_uid, 'ALERTA',
                                'Solicitud rechazada',
                                respuesta[:200],
                                '/portal-terceros'
                            )
                        refresh_callback()

                with ui.row().classes('w-full gap-2 justify-end'):
                    ui.button('Rechazar', on_click=rechazar, icon='close').props('flat').classes('text-red-600')
                    ui.button('Aprobar', on_click=aprobar, icon='check').props('unelevated').classes('bg-green-600')

    # Cargar inicial
    cargar_solicitudes()

    # Botón de refrescar
    with ui.row().classes('w-full justify-end mt-4'):
        ui.button('Refrescar', on_click=cargar_solicitudes, icon='refresh').props('flat')
