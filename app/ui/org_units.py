"""
Organizational Units — CRUD for UnidadOrganizacional hierarchy.
"""
from nicegui import ui
from app.ui.ui_helpers import estado_vacio
from app.core.database import get_db_session
from app.models.meta_tables import UnidadOrganizacional, Usuario


def page_org_units():
    ui.label('Unidades Organizacionales').classes('text-3xl font-bold text-slate-800 mb-2')
    ui.label('Gestiona la estructura jerárquica de la organización').classes('text-slate-500 mb-6')

    content = ui.column().classes('w-full gap-4')

    def contar_usuarios(unidad_id: int, db) -> int:
        return db.query(Usuario).filter_by(unidad_id=unidad_id).count()

    def cargar():
        content.clear()
        with get_db_session() as db:
            unidades = db.query(UnidadOrganizacional).order_by(
                UnidadOrganizacional.nivel, UnidadOrganizacional.nombre
            ).all()

            rows = []
            for u in unidades:
                padre_nombre = u.padre.nombre if u.padre else '—'
                n_usuarios = contar_usuarios(u.id, db)
                rows.append({
                    'id': u.id,
                    'nombre': u.nombre,
                    'codigo': u.codigo or '—',
                    'nivel': u.nivel,
                    'padre': padre_nombre,
                    'usuarios': n_usuarios,
                    'activo': u.activo,
                    'color': 'green' if u.activo else 'grey',
                })

        with content:
            cols = [
                {'name': 'nombre', 'label': 'Nombre', 'field': 'nombre', 'align': 'left', 'sortable': True},
                {'name': 'codigo', 'label': 'Código', 'field': 'codigo', 'align': 'left'},
                {'name': 'nivel', 'label': 'Nivel', 'field': 'nivel', 'align': 'center'},
                {'name': 'padre', 'label': 'Unidad Padre', 'field': 'padre', 'align': 'left'},
                {'name': 'usuarios', 'label': 'Usuarios', 'field': 'usuarios', 'align': 'center'},
                {'name': 'activo', 'label': 'Estado', 'field': 'activo', 'align': 'center'},
                {'name': 'acciones', 'label': 'Acciones', 'field': 'acciones', 'align': 'center'},
            ]
            tabla = ui.table(columns=cols, rows=rows, pagination=20).classes('w-full shadow-sm')
            if not rows:
                tabla.visible = False
                estado_vacio(
                    'account_tree',
                    'Aún no tienes unidades organizacionales',
                    'Las unidades reflejan la estructura de tu entidad (áreas, '
                    'direcciones, dependencias). Sirven para agrupar usuarios y '
                    'delegar responsabilidades. Crea la primera cuando la necesites.',
                    'Crear primera unidad',
                    lambda: abrir_dialog())
            tabla.add_slot('body-cell-activo', '''
                <q-td :props="props">
                    <q-badge :color="props.row.color">{{ props.row.activo ? "Activo" : "Inactivo" }}</q-badge>
                </q-td>
            ''')
            tabla.add_slot('body-cell-acciones', '''
                <q-td :props="props">
                    <q-btn flat round icon="edit" size="sm" color="primary"
                        @click="$emit(\'editar\', props.row)" />
                    <q-btn flat round icon="delete" size="sm" color="negative"
                        @click="$emit(\'eliminar\', props.row)" />
                </q-td>
            ''')
            tabla.on('editar', lambda e: abrir_dialog(e.args))
            tabla.on('eliminar', lambda e: confirmar_eliminar(e.args))

    def abrir_dialog(fila=None):
        es_nueva = fila is None
        form = {
            'id': fila['id'] if fila else None,
            'nombre': fila['nombre'] if fila else '',
            'codigo': (fila['codigo'] if fila and fila['codigo'] != '—' else ''),
            'descripcion': '',
            'padre_id': None,
            'nivel': fila['nivel'] if fila else 1,
            'activo': fila['activo'] if fila else True,
        }

        with ui.dialog() as d, ui.card().classes('min-w-[500px] p-6'):
            ui.label('Nueva Unidad' if es_nueva else 'Editar Unidad').classes('text-xl font-bold mb-4')

            with get_db_session() as db:
                otras = db.query(UnidadOrganizacional).filter_by(activo=True).all()
                opciones_padre = {None: 'Raíz (sin padre)'}
                opciones_padre.update({u.id: u.nombre for u in otras if (fila is None or u.id != fila['id'])})

            ui.input('Nombre *').bind_value(form, 'nombre').classes('w-full').props('outlined')
            ui.input('Código (opcional)').bind_value(form, 'codigo').classes('w-full mt-2').props('outlined')
            ui.select(opciones_padre, label='Unidad Padre').bind_value(form, 'padre_id').classes('w-full mt-2')
            ui.number('Nivel jerárquico', min=1, max=10).bind_value(form, 'nivel').classes('w-full mt-2').props('outlined')
            ui.checkbox('Activo').bind_value(form, 'activo')

            def guardar():
                if not form['nombre']:
                    ui.notify('El nombre es obligatorio', type='warning')
                    return
                with get_db_session() as db:
                    if es_nueva:
                        db.add(UnidadOrganizacional(
                            nombre=form['nombre'],
                            codigo=form['codigo'] or None,
                            padre_id=form['padre_id'],
                            nivel=int(form['nivel'] or 1),
                            activo=form['activo']
                        ))
                    else:
                        u = db.query(UnidadOrganizacional).get(form['id'])
                        if u:
                            u.nombre = form['nombre']
                            u.codigo = form['codigo'] or None
                            u.padre_id = form['padre_id']
                            u.nivel = int(form['nivel'] or 1)
                            u.activo = form['activo']
                    db.commit()
                ui.notify('Guardado', type='positive')
                d.close()
                cargar()

            with ui.row().classes('w-full justify-end gap-2 mt-4'):
                ui.button('Cancelar', on_click=d.close).props('flat')
                ui.button('Guardar', icon='save', on_click=guardar).classes('bg-green-600 text-white')

        d.open()

    def confirmar_eliminar(fila):
        with get_db_session() as db:
            n_usuarios = contar_usuarios(fila['id'], db)
            n_hijos = db.query(UnidadOrganizacional).filter_by(padre_id=fila['id']).count()

        if n_usuarios > 0 or n_hijos > 0:
            ui.notify(
                f'No se puede eliminar: tiene {n_usuarios} usuarios y {n_hijos} sub-unidades asignadas',
                type='warning', multi_line=True
            )
            return

        with ui.dialog() as d, ui.card().classes('p-6'):
            ui.label(f'¿Eliminar "{fila["nombre"]}"?').classes('font-bold')
            ui.label('Esta acción es irreversible.').classes('text-slate-500 text-sm')

            def confirmar():
                with get_db_session() as db:
                    u = db.query(UnidadOrganizacional).get(fila['id'])
                    if u:
                        db.delete(u)
                        db.commit()
                ui.notify('Unidad eliminada', type='negative')
                d.close()
                cargar()

            with ui.row().classes('w-full justify-end gap-2 mt-4'):
                ui.button('Cancelar', on_click=d.close).props('flat')
                ui.button('Eliminar', icon='delete', on_click=confirmar).classes('bg-red-600 text-white')

        d.open()

    with ui.row().classes('w-full justify-end mb-4'):
        ui.button('Nueva Unidad', icon='add', on_click=lambda: abrir_dialog()).classes('bg-blue-600 text-white')

    cargar()
