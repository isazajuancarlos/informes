"""
Creación Manual de Entregas (A Demanda)
Permite al administrador crear entregas sin esperar al scheduler automático
"""

from nicegui import ui, app
from app.ui.ui_helpers import estado_vacio
from app.core.database import get_db_session
from app.models.meta_tables import Informe, Usuario, EntregaEsperada
from datetime import datetime, timedelta

def page_entregas_manuales():
    """Página para crear entregas manuales"""

    # Estado
    form = {
        'informe_id': None,
        'usuario_id': None,
        'fecha_programada': datetime.now().strftime('%Y-%m-%d'),
        'hora_limite': '23:59',
        'notas': ''
    }

    def limpiar():
        form.update({
            'informe_id': None,
            'usuario_id': None,
            'fecha_programada': datetime.now().strftime('%Y-%m-%d'),
            'hora_limite': '23:59',
            'notas': ''
        })

    def crear_entrega():
        """Crea una entrega manual"""
        if not form['informe_id'] or not form['usuario_id']:
            ui.notify('Seleccione informe y usuario responsable', type='warning')
            return

        try:
            # Parsear fecha y hora
            fecha_prog = datetime.strptime(form['fecha_programada'], '%Y-%m-%d')
            hora_partes = form['hora_limite'].split(':')
            fecha_limite = fecha_prog.replace(hour=int(hora_partes[0]), minute=int(hora_partes[1]))

            with get_db_session() as db:
                # Verificar que no exista ya una entrega pendiente igual
                existe = db.query(EntregaEsperada).filter(
                    EntregaEsperada.informe_id == int(form['informe_id']),
                    EntregaEsperada.usuario_id == int(form['usuario_id']),
                    EntregaEsperada.fecha_programada == fecha_prog,
                    EntregaEsperada.estado == 'PENDIENTE'
                ).first()

                if existe:
                    ui.notify('Ya existe una entrega pendiente para este informe, usuario y fecha', type='warning')
                    return

                # Obtener usuario actual (admin)
                creador_id = app.storage.user.get('user_id')

                # Crear entrega
                entrega = EntregaEsperada(
                    informe_id=int(form['informe_id']),
                    usuario_id=int(form['usuario_id']),
                    fecha_programada=fecha_prog,
                    fecha_limite_teorica=fecha_limite,
                    estado='PENDIENTE',
                    es_manual=True,
                    creador_manual_id=creador_id,
                    notas_admin=form['notas'] if form['notas'] else None
                )

                db.add(entrega)
                db.commit()

                # Obtener datos para notificaciones
                informe = db.query(Informe).get(int(form['informe_id']))
                usuario = db.query(Usuario).get(int(form['usuario_id']))
                informe_nombre = informe.nombre if informe else '?'
                usuario_email = usuario.email if usuario else None
                usuario_nombre = usuario.nombre if usuario else '?'

                ui.notify(
                    f'✅ Entrega creada: "{informe_nombre}" → {usuario_nombre}',
                    type='positive',
                    close_button=True
                )

                # Send email and in-app notification
                import asyncio
                from app.services.email_service import EmailService
                from app.services.notification_service import NotificationService

                if usuario_email:
                    asyncio.create_task(EmailService.enviar_asignacion_manual(
                        usuario_email, informe_nombre, fecha_limite.strftime('%Y-%m-%d %H:%M'),
                        form['notas'] if form['notas'] else None
                    ))
                NotificationService.crear(
                    int(form['usuario_id']), 'INFO',
                    f"Nueva tarea: {informe_nombre}",
                    f"Límite: {fecha_limite.strftime('%d/%m/%Y %H:%M')}",
                    '/mis-pendientes'
                )

                limpiar()
                cargar_entregas_manuales()

        except ValueError as e:
            ui.notify(f'Error en formato de fecha/hora: {e}', type='negative')
        except Exception as e:
            ui.notify(f'Error al crear entrega: {e}', type='negative')

    def cargar_entregas_manuales():
        """Carga lista de entregas manuales recientes"""
        with get_db_session() as db:
            entregas = db.query(EntregaEsperada).filter(
                EntregaEsperada.es_manual == True
            ).order_by(EntregaEsperada.fecha_programada.desc()).limit(50).all()

            rows = []
            for e in entregas:
                color = {
                    'PENDIENTE': 'orange',
                    'CUMPLIDO': 'green',
                    'EXTEMPORANEO': 'yellow',
                    'INCUMPLIDO': 'red'
                }.get(e.estado, 'grey')

                rows.append({
                    'id': e.id,
                    'informe': e.informe.nombre if e.informe else '-',
                    'usuario': e.usuario.nombre if e.usuario else '-',
                    'fecha_prog': e.fecha_programada.strftime('%Y-%m-%d'),
                    'fecha_limite': e.fecha_limite_teorica.strftime('%Y-%m-%d %H:%M') if e.fecha_limite_teorica else '-',
                    'estado': e.estado,
                    'color': color,
                    'creador': e.creador_manual.nombre if e.creador_manual else '-',
                    'notas': e.notas_admin or '-'
                })

            tabla.rows = rows
            tabla.visible = bool(rows)
            vacio.visible = not rows

    # UI Principal
    ui.label('Crear Entrega Manual').classes('text-3xl font-bold text-slate-800 mb-2')
    ui.label('Para informes "A Demanda" o excepciones').classes('text-slate-500 mb-6')

    with ui.card().classes('w-full p-6 mb-6'):
        ui.label('Nueva Entrega Manual').classes('text-xl font-bold mb-4')

        with ui.row().classes('w-full gap-4'):
            # Selector de informe
            with ui.column().classes('flex-grow'):
                with get_db_session() as db:
                    informes = db.query(Informe).filter_by(activo=True).order_by(Informe.nombre).all()
                    opciones_informes = {str(i.id): i.nombre for i in informes}

                ui.select(
                    label='Informe *',
                    options=opciones_informes,
                    with_input=True
                ).bind_value(form, 'informe_id').classes('w-full').props('outlined')

            # Selector de usuario
            with ui.column().classes('flex-grow'):
                with get_db_session() as db:
                    usuarios = db.query(Usuario).filter_by(activo=True).order_by(Usuario.nombre).all()
                    opciones_usuarios = {str(u.id): f"{u.nombre} ({u.email})" for u in usuarios}

                ui.select(
                    label='Responsable *',
                    options=opciones_usuarios,
                    with_input=True
                ).bind_value(form, 'usuario_id').classes('w-full').props('outlined')

        with ui.row().classes('w-full gap-4 mt-4'):
            ui.input('Fecha Programada', value=form['fecha_programada']).bind_value(form, 'fecha_programada') \
                .classes('w-1/3').props('outlined type=date')

            ui.input('Hora Límite', value=form['hora_limite']).bind_value(form, 'hora_limite') \
                .classes('w-1/3').props('outlined type=time')

        ui.textarea('Notas (Opcional)').bind_value(form, 'notas') \
            .classes('w-full mt-4').props('outlined rows=2 placeholder="Ej: Creada por solicitud urgente del director"')

        with ui.row().classes('w-full justify-end gap-2 mt-4'):
            ui.button('Limpiar', on_click=limpiar, icon='clear').props('flat')
            ui.button('Crear Entrega', on_click=crear_entrega, icon='add_task') \
                .classes('bg-blue-600 text-white')

    # Tabla de entregas manuales recientes
    ui.label('Entregas Manuales Recientes').classes('text-2xl font-bold text-slate-800 mt-8 mb-4')

    cols = [
        {'name': 'fecha_prog', 'label': 'Fecha Programada', 'field': 'fecha_prog', 'align': 'left', 'sortable': True},
        {'name': 'informe', 'label': 'Informe', 'field': 'informe', 'align': 'left'},
        {'name': 'usuario', 'label': 'Responsable', 'field': 'usuario', 'align': 'left'},
        {'name': 'fecha_limite', 'label': 'Límite', 'field': 'fecha_limite', 'align': 'left'},
        {'name': 'estado', 'label': 'Estado', 'field': 'estado', 'align': 'center'},
        {'name': 'creador', 'label': 'Creado Por', 'field': 'creador', 'align': 'left'},
        {'name': 'notas', 'label': 'Notas', 'field': 'notas', 'align': 'left'}
    ]

    tabla = ui.table(columns=cols, rows=[], pagination=20).classes('w-full shadow-sm')
    vacio = estado_vacio(
        'assignment_add',
        'Aún no has creado entregas manuales',
        'Una entrega manual asigna una tarea puntual fuera del cronograma '
        'automático —un requerimiento extraordinario, una corrección—. Créala con '
        'el formulario de arriba cuando lo necesites.')

    # Slot para estado
    tabla.add_slot('body-cell-estado', '''
        <q-td :props="props">
            <q-badge :color="props.row.color">
                {{ props.row.estado }}
            </q-badge>
        </q-td>
    ''')

    # Cargar datos
    cargar_entregas_manuales()
