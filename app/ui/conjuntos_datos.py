"""
Gestión de Conjuntos de Datos
Módulo para administrar catálogo de conjuntos de datos (RIPS, RUAF, Vacunación, etc.)
"""

from nicegui import ui
from app.ui.ui_helpers import estado_vacio
from app.core.database import get_db_session
from app.models.meta_tables import ConjuntoDatos, Usuario, Informe

def page_conjuntos_datos():
    """Página de gestión de conjuntos de datos"""

    # Estado
    estado = {'id_edicion': None}
    form = {
        'nombre': '',
        'descripcion': '',
        'responsable_id': None,
        'activo': True
    }

    def limpiar():
        form.update({
            'nombre': '',
            'descripcion': '',
            'responsable_id': None,
            'activo': True
        })
        estado['id_edicion'] = None

    def cargar():
        with get_db_session() as db:
            conjuntos = db.query(ConjuntoDatos).order_by(ConjuntoDatos.nombre).all()
            rows = []

            for c in conjuntos:
                # Contar informes asociados
                count_informes = db.query(Informe).filter_by(conjunto_datos_id=c.id).count()

                rows.append({
                    'id': c.id,
                    'nombre': c.nombre,
                    'descripcion': c.descripcion or '-',
                    'responsable': c.responsable.nombre if c.responsable else 'Sin asignar',
                    'responsable_id': c.responsable_id,
                    'activo': c.activo,
                    'estado_txt': 'Activo' if c.activo else 'Inactivo',
                    'count_informes': count_informes
                })

            tabla.rows = rows
            tabla.visible = bool(rows)
            vacio.visible = not rows

    def abrir_edicion(row):
        limpiar()
        estado['id_edicion'] = row['id']
        form.update({
            'nombre': row['nombre'],
            'descripcion': row['descripcion'],
            'responsable_id': str(row['responsable_id']) if row['responsable_id'] else None,
            'activo': row['activo']
        })
        dialogo.open()

    def eliminar(id_conjunto):
        with get_db_session() as db:
            # Verificar si tiene informes asociados
            count_informes = db.query(Informe).filter_by(conjunto_datos_id=id_conjunto).count()

            if count_informes > 0:
                ui.notify(
                    f'No se puede eliminar: Tiene {count_informes} informes asociados',
                    type='negative',
                    close_button=True
                )
                return

            conjunto = db.query(ConjuntoDatos).get(id_conjunto)
            if conjunto:
                db.delete(conjunto)
                db.commit()
                ui.notify('Conjunto de datos eliminado', type='positive')
                cargar()
            else:
                ui.notify('No se encontró el conjunto', type='warning')

    def guardar():
        if not form['nombre']:
            ui.notify('El nombre es obligatorio', type='warning')
            return

        with get_db_session() as db:
            if estado['id_edicion']:
                # Editar
                conjunto = db.query(ConjuntoDatos).get(estado['id_edicion'])
                if conjunto:
                    conjunto.nombre = form['nombre']
                    conjunto.descripcion = form['descripcion']
                    conjunto.responsable_id = int(form['responsable_id']) if form['responsable_id'] else None
                    conjunto.activo = form['activo']
                    msg = "Conjunto actualizado"
                else:
                    ui.notify('Conjunto no encontrado', type='negative')
                    return
            else:
                # Crear
                if db.query(ConjuntoDatos).filter_by(nombre=form['nombre']).first():
                    ui.notify(f'Ya existe un conjunto con el nombre "{form["nombre"]}"', type='negative')
                    return

                conjunto = ConjuntoDatos(
                    nombre=form['nombre'],
                    descripcion=form['descripcion'],
                    responsable_id=int(form['responsable_id']) if form['responsable_id'] else None,
                    activo=form['activo']
                )
                db.add(conjunto)
                msg = "Conjunto creado"

            db.commit()

        ui.notify(msg, type='positive')
        dialogo.close()
        cargar()

    # UI Principal
    ui.label('Gestión de Conjuntos de Datos').classes('text-3xl font-bold text-slate-800 mb-2')
    ui.label('Catálogo de conjuntos de datos (RIPS, RUAF, Vacunación, etc.)').classes('text-slate-500 mb-6')

    # Tabla
    cols = [
        {'name': 'nombre', 'label': 'Nombre', 'field': 'nombre', 'align': 'left', 'sortable': True},
        {'name': 'descripcion', 'label': 'Descripción', 'field': 'descripcion', 'align': 'left'},
        {'name': 'responsable', 'label': 'Responsable', 'field': 'responsable', 'align': 'left'},
        {'name': 'informes', 'label': 'Informes', 'field': 'count_informes', 'align': 'center'},
        {'name': 'estado', 'label': 'Estado', 'field': 'estado_txt', 'align': 'center'},
        {'name': 'acciones', 'label': 'Acciones', 'field': 'acciones', 'align': 'center'}
    ]

    tabla = ui.table(columns=cols, rows=[], pagination=20).classes('w-full shadow-sm')

    # Slot para estado
    tabla.add_slot('body-cell-estado', '''
        <q-td :props="props">
            <q-icon :name="props.row.activo ? 'check_circle' : 'cancel'"
                    :color="props.row.activo ? 'green' : 'grey'"
                    size="sm" class="mr-1" />
            {{ props.row.estado_txt }}
        </q-td>
    ''')

    # Slot para informes
    tabla.add_slot('body-cell-informes', '''
        <q-td :props="props">
            <q-badge :color="props.row.count_informes > 0 ? 'blue' : 'grey'">
                {{ props.row.count_informes }}
            </q-badge>
        </q-td>
    ''')

    # Slot para acciones
    tabla.add_slot('body-cell-acciones', '''
        <q-td :props="props">
            <q-btn icon="edit" flat dense size="sm" color="primary"
                   @click="$parent.$emit('edit', props.row)">
                <q-tooltip>Editar</q-tooltip>
            </q-btn>
            <q-btn icon="delete" flat dense size="sm" color="negative"
                   @click="$parent.$emit('delete', props.row.id)">
                <q-tooltip>Eliminar</q-tooltip>
            </q-btn>
        </q-td>
    ''')

    tabla.on('edit', lambda e: abrir_edicion(e.args))
    tabla.on('delete', lambda e: eliminar(e.args))

    # Botón nuevo
    vacio = estado_vacio(
        'dataset',
        'Aún no tienes conjuntos de datos',
        'Un conjunto de datos agrupa registros relacionados para reutilizarlos en '
        'tus informes, en vez de recapturarlos cada vez. Crea el primero cuando lo necesites.',
        'Crear primer conjunto',
        lambda: [limpiar(), dialogo.open()],
    )
    ui.button('Nuevo Conjunto de Datos', icon='add', on_click=lambda: [limpiar(), dialogo.open()]) \
        .classes('mt-4 bg-slate-900 text-white')

    # Diálogo de edición
    with ui.dialog() as dialogo, ui.card().classes('min-w-[600px] p-6'):
        ui.label('Conjunto de Datos').classes('text-xl font-bold mb-4')

        ui.input('Nombre').bind_value(form, 'nombre').classes('w-full').props('outlined')
        ui.textarea('Descripción').bind_value(form, 'descripcion').classes('w-full').props('outlined rows=3')

        # Selector de responsable
        with get_db_session() as db:
            usuarios = db.query(Usuario).filter_by(activo=True).order_by(Usuario.nombre).all()
            opciones_usuarios = {str(u.id): u.nombre for u in usuarios}
            opciones_usuarios[''] = 'Sin asignar'

        ui.select(
            label='Responsable',
            options=opciones_usuarios,
            with_input=True
        ).bind_value(form, 'responsable_id').classes('w-full').props('outlined')

        with ui.row().classes('w-full items-center gap-2'):
            ui.checkbox('Activo').bind_value(form, 'activo')

        ui.separator().classes('my-4')

        with ui.row().classes('w-full justify-end gap-2'):
            ui.button('Cancelar', on_click=dialogo.close).props('flat')
            ui.button('Guardar', on_click=guardar, icon='save').classes('bg-blue-600 text-white')

    # Cargar datos
    cargar()
