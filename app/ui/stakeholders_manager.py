from nicegui import ui
from app.ui.ui_helpers import estado_vacio
from app.core.database import get_db_session
from app.models.meta_tables import Stakeholder, Usuario
from app.services.auth import AuthService
from app.services.email_service import EmailService

def page_stakeholders():
    form = {
        'nombre': '',
        'contacto': '',
        'email': '',
        'tipo': 'EXTERNO',
        'crear_usuario': True
    }
    estado = {'id_edicion': None}

    def guardar():
        if not form['nombre']:
            ui.notify('Nombre de Entidad obligatorio', type='warning')
            return

        if form['crear_usuario'] and not form['email']:
            ui.notify('El email es obligatorio para crear usuario', type='warning')
            return

        with get_db_session() as db:
            if estado['id_edicion']:
                # --- MODO EDICIÓN ---
                s = db.get(Stakeholder, estado['id_edicion'])
                if not s:
                    ui.notify('Tercero no encontrado', type='negative')
                    return
                s.nombre = form['nombre']
                s.contacto_nombre = form['contacto']
                s.email = form['email']
                s.tipo = form['tipo']
                db.commit()
                msg = "Tercero actualizado"
                estado['id_edicion'] = None
                label_titulo.text = 'Nuevo Tercero'
            else:
                # --- MODO CREACIÓN ---
                if form['crear_usuario']:
                    if db.query(Usuario).filter_by(email=form['email']).first():
                        ui.notify(f'El usuario {form["email"]} ya existe', type='negative')
                        return

                s = Stakeholder(
                    nombre=form['nombre'],
                    contacto_nombre=form['contacto'],
                    email=form['email'],
                    tipo=form['tipo']
                )
                db.add(s)
                db.flush()

                msg = "Tercero creado"

                if form['crear_usuario']:
                    pass_temp = "123456"
                    pass_hash = AuthService.get_password_hash(pass_temp)

                    u = Usuario(
                        nombre=form['nombre'],
                        email=form['email'],
                        password_hash=pass_hash,
                        es_admin=False,
                        rol_sistema='TERCERO',
                        activo=True
                    )
                    db.add(u)
                    db.flush()

                    s.usuario_id = u.id
                    msg += f" y Usuario generado (Pass: {pass_temp})"

                db.commit()

        ui.notify(msg, type='positive', close_button=True, multi_line=True)
        cargar_tabla()
        form.update({'nombre': '', 'contacto': '', 'email': '', 'tipo': 'EXTERNO', 'crear_usuario': True})
        estado['id_edicion'] = None
        label_titulo.text = 'Nuevo Tercero'

    def editar(fila):
        estado['id_edicion'] = fila['id']
        form['nombre'] = fila['nombre']
        form['contacto'] = fila['contacto'] if fila['contacto'] != '---' else ''
        form['email'] = fila['email'] if fila['email'] != '---' else ''
        form['tipo'] = fila['tipo']
        form['crear_usuario'] = False
        label_titulo.text = f'Editar Tercero (ID: {fila["id"]})'

    def confirmar_eliminar(fila):
        def eliminar():
            with get_db_session() as db:
                s = db.get(Stakeholder, fila['id'])
                if s:
                    db.delete(s)
                    db.commit()
            ui.notify('Tercero eliminado', type='warning')
            d.close()
            cargar_tabla()

        with ui.dialog() as d, ui.card():
            ui.label(f'¿Eliminar "{fila["nombre"]}"?').classes('text-lg font-bold')
            ui.label('Esta acción no se puede deshacer.').classes('text-sm text-gray-500')
            with ui.row().classes('w-full justify-end mt-4'):
                ui.button('Cancelar', on_click=d.close).props('flat')
                ui.button('Eliminar', color='red', on_click=eliminar)
        d.open()

    def cargar_tabla():
        tabla.rows = []
        with get_db_session() as db:
            data = db.query(Stakeholder).all()
            rows = []
            for s in data:
                rows.append({
                    'id': s.id,
                    'nombre': s.nombre,
                    'contacto': s.contacto_nombre or '---',
                    'email': s.email or '---',
                    'tipo': s.tipo,
                    'acceso': "Sí" if s.usuario_id else "No"
                })
            tabla.rows = rows
            tabla.visible = bool(rows)
            vacio.visible = not rows

    ui.label('Directorio de Terceros (Stakeholders)').classes('text-2xl font-bold text-slate-800 mb-4')

    with ui.row().classes('w-full gap-6 items-start'):
        with ui.card().classes('w-1/3 p-4'):
            label_titulo = ui.label('Nuevo Tercero').classes('text-lg font-bold text-blue-800 mb-2')
            ui.input('Nombre Entidad').bind_value(form, 'nombre').classes('w-full')
            ui.input('Nombre Contacto').bind_value(form, 'contacto').classes('w-full')
            ui.input('Email').bind_value(form, 'email').classes('w-full')
            ui.label('Tipo').classes('text-xs font-bold mt-2 text-gray-500')
            ui.select(['INTERNO', 'EXTERNO'], value='EXTERNO').bind_value(form, 'tipo').classes('w-full')
            ui.separator().classes('my-4')
            ui.checkbox('Crear Usuario de Acceso').bind_value(form, 'crear_usuario').props('color=green')
            ui.label('Genera usuario con contraseña "123456" y rol TERCERO').classes('text-xs text-gray-400 ml-8')
            with ui.row().classes('w-full gap-2 mt-4'):
                ui.button('Guardar', on_click=guardar).classes('flex-grow bg-slate-900 text-white')
                ui.button('Cancelar', on_click=lambda: [
                    form.update({'nombre': '', 'contacto': '', 'email': '', 'tipo': 'EXTERNO', 'crear_usuario': True}),
                    estado.update({'id_edicion': None}),
                    label_titulo.set_text('Nuevo Tercero')
                ]).props('flat color=grey')

        with ui.column().classes('w-2/3'):
            cols = [
                {'name': 'nombre', 'label': 'Entidad', 'field': 'nombre', 'align': 'left'},
                {'name': 'contacto', 'label': 'Contacto', 'field': 'contacto', 'align': 'left'},
                {'name': 'email', 'label': 'Email', 'field': 'email', 'align': 'left'},
                {'name': 'tipo', 'label': 'Tipo', 'field': 'tipo', 'align': 'center'},
                {'name': 'acceso', 'label': 'Acceso', 'field': 'acceso', 'align': 'center'},
                {'name': 'acciones', 'label': 'Acciones', 'field': 'acciones', 'align': 'center'},
            ]
            tabla = ui.table(columns=cols, rows=[]).classes('w-full shadow-sm border rounded')
            tabla.add_slot('body-cell-tipo', '<q-td :props="props"><q-badge :color="props.row.tipo == \'INTERNO\' ? \'blue\' : \'orange\'">{{ props.row.tipo }}</q-badge></q-td>')
            tabla.add_slot('body-cell-acceso', '<q-td :props="props"><q-icon :name="props.row.acceso == \'Sí\' ? \'vpn_key\' : \'block\'" :color="props.row.acceso == \'Sí\' ? \'green\' : \'grey\'" size="sm" /></q-td>')
            tabla.add_slot('body-cell-acciones', '''
                <q-td :props="props">
                    <q-btn icon="edit" color="primary" flat dense size="sm" @click="$parent.$emit('editar', props.row)" />
                    <q-btn icon="delete" color="negative" flat dense size="sm" @click="$parent.$emit('borrar', props.row)" />
                </q-td>
            ''')
            tabla.on('editar', lambda e: editar(e.args))
            tabla.on('borrar', lambda e: confirmar_eliminar(e.args))
            # El formulario de alta está siempre visible a la izquierda, así que
            # el estado vacío solo explica qué es un tercero (sin botón).
            vacio = estado_vacio(
                'contacts',
                'Aún no hay terceros registrados',
                'Los terceros son las entidades con las que interactúas: entes de '
                'control que solicitan informes, o destinatarios a quienes los envías. '
                'Regístralos con el formulario de la izquierda.')
            cargar_tabla()
