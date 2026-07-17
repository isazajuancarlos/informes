"""
Delegaciones — permite delegar tareas entre usuarios con rango de fechas.
"""
from nicegui import ui, app
from datetime import datetime, date
from app.core.database import get_db_session
from app.models.meta_tables import Delegacion, Usuario


def page_delegation():
    es_admin = app.storage.user.get('es_admin', False)
    uid_actual = app.storage.user.get('user_id')

    ui.label('Delegaciones').classes('text-3xl font-bold text-slate-800 mb-2')
    ui.label('Transfiere temporalmente responsabilidades entre operadores').classes('text-slate-500 mb-6')

    content = ui.column().classes('w-full')

    def cargar():
        content.clear()
        with get_db_session() as db:
            q = db.query(Delegacion)
            if not es_admin:
                q = q.filter(
                    (Delegacion.delegante_id == uid_actual) | (Delegacion.delegado_id == uid_actual)
                )
            delegaciones = q.order_by(Delegacion.fecha_inicio.desc()).all()

            rows = []
            for d in delegaciones:
                rows.append({
                    'id': d.id,
                    'delegante': d.delegante.nombre if d.delegante else '?',
                    'delegado': d.delegado.nombre if d.delegado else '?',
                    'desde': d.fecha_inicio.strftime('%Y-%m-%d'),
                    'hasta': d.fecha_fin.strftime('%Y-%m-%d'),
                    'motivo': d.motivo or '—',
                    'activo': d.activo,
                    'color': 'green' if d.activo else 'grey',
                })

        with content:
            cols = [
                {'name': 'delegante', 'label': 'Delegante', 'field': 'delegante', 'align': 'left'},
                {'name': 'delegado', 'label': 'Delegado', 'field': 'delegado', 'align': 'left'},
                {'name': 'desde', 'label': 'Desde', 'field': 'desde', 'align': 'center'},
                {'name': 'hasta', 'label': 'Hasta', 'field': 'hasta', 'align': 'center'},
                {'name': 'motivo', 'label': 'Motivo', 'field': 'motivo', 'align': 'left'},
                {'name': 'activo', 'label': 'Estado', 'field': 'activo', 'align': 'center'},
                {'name': 'acciones', 'label': 'Acciones', 'field': 'acciones', 'align': 'center'},
            ]
            tabla = ui.table(columns=cols, rows=rows, pagination=20).classes('w-full shadow-sm')
            tabla.add_slot('body-cell-activo', '''
                <q-td :props="props">
                    <q-badge :color="props.row.color">{{ props.row.activo ? "Activa" : "Cancelada" }}</q-badge>
                </q-td>
            ''')
            tabla.add_slot('body-cell-acciones', '''
                <q-td :props="props">
                    <q-btn v-if="props.row.activo" flat round icon="cancel" size="sm" color="negative"
                        @click="$emit(\'cancelar\', props.row)" />
                </q-td>
            ''')
            tabla.on('cancelar', lambda e: cancelar_delegacion(e.args['id']))

    def cancelar_delegacion(del_id: int):
        with get_db_session() as db:
            d = db.query(Delegacion).get(del_id)
            if d:
                d.activo = False
                db.commit()
        ui.notify('Delegación cancelada', type='warning')
        cargar()

    def abrir_dialog():
        form = {
            'delegante_id': uid_actual if not es_admin else None,
            'delegado_id': None,
            'fecha_inicio': date.today().isoformat(),
            'fecha_fin': date.today().isoformat(),
            'motivo': '',
        }

        with get_db_session() as db:
            usuarios = db.query(Usuario).filter_by(activo=True).order_by(Usuario.nombre).all()
            opts_usuarios = {u.id: f"{u.nombre} ({u.email})" for u in usuarios}

        with ui.dialog() as d, ui.card().classes('min-w-[500px] p-6'):
            ui.label('Nueva Delegación').classes('text-xl font-bold mb-4')

            if es_admin:
                ui.select(opts_usuarios, label='Delegante (quien delega)').bind_value(form, 'delegante_id').classes('w-full')
            else:
                ui.label(f'Delegante: {app.storage.user.get("nombre", "")}').classes('text-slate-600 mb-2')

            ui.select(opts_usuarios, label='Delegado (quien recibe)', with_input=True) \
                .bind_value(form, 'delegado_id').classes('w-full mt-2')

            with ui.row().classes('w-full gap-4 mt-2'):
                ui.input('Desde').props('type=date').bind_value(form, 'fecha_inicio').classes('flex-grow')
                ui.input('Hasta').props('type=date').bind_value(form, 'fecha_fin').classes('flex-grow')

            ui.textarea('Motivo (opcional)', placeholder='Ej: Vacaciones, enfermedad...') \
                .bind_value(form, 'motivo').classes('w-full mt-2').props('outlined rows=2')

            def guardar():
                if not form['delegado_id']:
                    ui.notify('Selecciona el delegado', type='warning')
                    return
                if form['delegante_id'] == form['delegado_id']:
                    ui.notify('El delegante y el delegado deben ser distintos', type='warning')
                    return
                try:
                    fi = datetime.strptime(form['fecha_inicio'], '%Y-%m-%d')
                    ff = datetime.strptime(form['fecha_fin'], '%Y-%m-%d')
                    if ff < fi:
                        ui.notify('La fecha de fin debe ser posterior al inicio', type='warning')
                        return
                except ValueError:
                    ui.notify('Formato de fecha inválido', type='warning')
                    return

                with get_db_session() as db:
                    db.add(Delegacion(
                        delegante_id=form['delegante_id'],
                        delegado_id=form['delegado_id'],
                        fecha_inicio=fi,
                        fecha_fin=ff,
                        motivo=form['motivo'] or None,
                        activo=True
                    ))
                    db.commit()

                ui.notify('Delegación creada', type='positive')
                d.close()
                cargar()

            with ui.row().classes('w-full justify-end gap-2 mt-4'):
                ui.button('Cancelar', on_click=d.close).props('flat')
                ui.button('Crear', icon='add', on_click=guardar).classes('bg-blue-600 text-white')

        d.open()

    with ui.row().classes('w-full justify-end mb-4'):
        ui.button('Nueva Delegación', icon='add', on_click=abrir_dialog).classes('bg-blue-600 text-white')

    cargar()
