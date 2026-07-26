from nicegui import ui
from app.ui.ui_helpers import estado_vacio
from app.core.database import get_db_session
from app.models.meta_tables import Usuario, Stakeholder, UnidadOrganizacional
from app.services.auth import AuthService

def page_users_manager():
    estado = {'id_edicion': None}
    form = {
        'nombre': '', 'email': '', 'password': '',
        'rol_sistema': 'OPERADOR',
        'activo': True,
        'unidad_id': None,
    }

    def limpiar():
        form.update({'nombre':'', 'email':'', 'password':'', 'rol_sistema':'OPERADOR', 'activo':True, 'unidad_id': None})
        estado['id_edicion'] = None
        input_pass.props('placeholder="Requerido para nuevos"')

    def cargar():
        with get_db_session() as db:
            users = db.query(Usuario).order_by(Usuario.id).all()
            rows = []
            for u in users:
                # Lógica visual de rol
                rol_show = u.rol_sistema
                if u.es_admin: rol_show = 'ADMIN'
                # Check tercero
                if db.query(Stakeholder).filter_by(usuario_id=u.id).first(): rol_show = 'TERCERO'

                rows.append({
                    'id': u.id, 'nombre': u.nombre, 'email': u.email,
                    'rol': rol_show,
                    'estado': 'Activo' if u.activo else 'Inactivo',
                    'activo': u.activo, 'rol_db': u.rol_sistema, 'admin': u.es_admin,
                    'unidad': u.unidad.nombre if u.unidad else '—',
                    'unidad_id': u.unidad_id,
                })
            tabla.rows = rows
            tabla.visible = bool(rows)
            vacio.visible = not rows

    def abrir_edicion(row):
        limpiar()
        estado['id_edicion'] = row['id']
        form.update({
            'nombre': row['nombre'], 'email': row['email'],
            'rol_sistema': row['rol_db'], 'activo': row['activo'],
            'unidad_id': row.get('unidad_id'),
        })
        input_pass.props('placeholder="Vacío para mantener"')
        if row['rol'] == 'TERCERO':
            ui.notify('Usuario Tercero. Edite en módulo Terceros.', type='info')
        dialogo.open()

    def guardar():
        if not form['nombre'] or not form['email']: ui.notify('Datos incompletos', type='warning'); return
        with get_db_session() as db:
            es_admin = (form['rol_sistema'] == 'ADMIN')
            
            if estado['id_edicion']:
                u = db.get(Usuario, estado['id_edicion'])
                u.nombre = form['nombre']
                u.email = form['email']
                u.rol_sistema = form['rol_sistema']
                u.es_admin = es_admin
                u.activo = form['activo']
                u.unidad_id = form['unidad_id']
                if form['password']:
                    u.password_hash = AuthService.get_password_hash(form['password'])
                msg = "Actualizado"
            else:
                if db.query(Usuario).filter_by(email=form['email']).first():
                    ui.notify('Email duplicado', type='negative')
                    return
                db.add(Usuario(
                    nombre=form['nombre'], email=form['email'],
                    password_hash=AuthService.get_password_hash(form['password']),
                    rol_sistema=form['rol_sistema'], es_admin=es_admin,
                    activo=form['activo'], unidad_id=form['unidad_id']
                ))
                msg = "Creado"
            db.commit()
        ui.notify(msg, type='positive'); dialogo.close(); cargar()

    ui.label('Gestión de Usuarios').classes('text-2xl font-bold mb-4')
    cols = [{'name':'nombre','label':'Nombre','field':'nombre','align':'left'}, {'name':'email','label':'Email','field':'email','align':'left'}, {'name':'rol','label':'Rol','field':'rol','align':'left'}, {'name':'unidad','label':'Unidad','field':'unidad','align':'left'}, {'name':'est','label':'Estado','field':'estado','align':'center'}, {'name':'act','label':'Acción','field':'act','align':'center'}]
    tabla = ui.table(columns=cols, rows=[], pagination=10).classes('w-full shadow-sm')
    tabla.add_slot('body-cell-rol', '<q-td :props="props"><q-badge :color="props.row.rol==\'ADMIN\'?\'purple\':(props.row.rol==\'SUPERVISOR\'?\'teal\':\'blue\')">{{props.row.rol}}</q-badge></q-td>')
    tabla.add_slot('body-cell-est', '<q-td :props="props"><q-icon :name="props.row.activo?\'check_circle\':\'cancel\'" :color="props.row.activo?\'green\':\'grey\'" size="sm" class="mr-1"/>{{props.row.estado}}</q-td>')
    tabla.add_slot('body-cell-act', '<q-td :props="props"><q-btn icon="edit" flat dense size="sm" color="primary" @click="$parent.$emit(\'edit\', props.row)" /></q-td>')
    tabla.on('edit', lambda e: abrir_edicion(e.args))
    
    vacio = estado_vacio(
        'group',
        'Aún no hay más usuarios',
        'Los operadores registran las entregas del día a día; los supervisores '
        'consultan los tableros. Invita a tu equipo para repartir el trabajo en '
        'vez de cargarlo todo en el administrador.',
        'Crear primer usuario',
        lambda: [limpiar(), dialogo.open()],
    )

    ui.button('Nuevo Usuario', icon='person_add', on_click=lambda: [limpiar(), dialogo.open()]).classes('mt-4 bg-slate-900 text-white')

    with ui.dialog() as dialogo, ui.card().classes('min-w-[450px] p-6'):
        ui.label('Usuario').classes('text-xl font-bold mb-4')
        ui.input('Nombre').bind_value(form, 'nombre').classes('w-full')
        ui.input('Email').bind_value(form, 'email').classes('w-full')
        input_pass = ui.input('Contraseña', password=True).bind_value(form, 'password').classes('w-full')
        ui.separator().classes('my-4')
        
        # SELECTOR DE ROL ACTUALIZADO
        ui.label('Rol en el Sistema').classes('font-bold text-sm')
        ui.select(['ADMIN', 'SUPERVISOR', 'OPERADOR'], label='Seleccione Rol') \
            .bind_value(form, 'rol_sistema').classes('w-full')
        
        ui.checkbox('Usuario Activo').bind_value(form, 'activo').classes('mt-2')

        ui.separator().classes('my-2')
        with get_db_session() as _db:
            unidades = _db.query(UnidadOrganizacional).filter_by(activo=True).order_by(UnidadOrganizacional.nombre).all()
            opts_u = {None: '— Sin unidad —'}
            opts_u.update({u.id: u.nombre for u in unidades})
        ui.select(opts_u, label='Unidad Organizacional').bind_value(form, 'unidad_id').classes('w-full')
        
        with ui.row().classes('w-full justify-end mt-6 gap-2'):
            ui.button('Cancelar', on_click=dialogo.close).props('flat')
            ui.button('Guardar', on_click=guardar).classes('bg-blue-600 text-white')
    
    cargar()