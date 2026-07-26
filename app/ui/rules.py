from nicegui import ui
from app.core.database import get_db_session
from app.models.meta_tables import MetaVariable, MetaRegla
from app.ui.ui_helpers import estado_vacio, ayuda

def page_rules():
    # --- 1. CARGA DE DATOS ---
    def obtener_opciones_vars():
        with get_db_session() as db:
            vars_db = db.query(MetaVariable).all()
            # Diccionario: ID -> "Pregunta Visual" (Para mostrar bonito)
            return {v.nombre_interno: f"[{v.nombre_interno}] {v.etiqueta_visual}" for v in vars_db}

    opciones_vars = obtener_opciones_vars()

    # Estado Reactivo
    regla = {
        'nombre': '',
        'trigger': None,
        'op': 'EQUALS',
        'val': '',
        'cons': 'FILE_REQ',
        'msg': '',
        'ext': [],
        'dias_ajuste': 0
    }

    # --- 2. ACCIONES ---
    def borrar_regla(id_regla):
        with get_db_session() as db:
            r = db.get(MetaRegla, id_regla)
            if r:
                db.delete(r)
                db.commit()
        ui.notify('Regla eliminada', type='negative')
        cargar_tabla()

    def cargar_tabla():
        with get_db_session() as db:
            reglas_db = db.query(MetaRegla).all()
            filas = []
            for r in reglas_db:
                # Obtener nombre bonito de la variable
                nombre_var = r.variable.etiqueta_visual if r.variable else r.variable_id
                
                # Limpiar visualización de formatos (de ['pdf'] a pdf)
                formatos = r.payload_json.get('formatos_permitidos', [])
                txt_formatos = ", ".join(formatos) if formatos else ""
                
                # Crear frase lógica legible
                logica_humanizada = f"Si <b>{nombre_var}</b> es <b>{r.operador}</b> a <b>{r.valor_comparacion_json}</b>"
                
                # Definir el efecto visualmente
                efecto = ""
                if r.tipo_consecuencia == 'FILE_REQ':
                    efecto = f"📄 Pedir Archivo ({txt_formatos})"
                elif r.tipo_consecuencia == 'NOTIFY_ROLE':
                    efecto = f"📧 Notificar a {r.payload_json.get('rol_destino', '?')}"
                elif r.tipo_consecuencia == 'DATE_CHANGE':
                    dias = r.payload_json.get('dias_ajuste', 0)
                    efecto = f"📅 Cambiar Fecha ({dias:+d} días)"
                elif r.tipo_consecuencia == 'BLOCK':
                    efecto = "🚫 Bloquear Proceso"
                
                filas.append({
                    'id': r.id,
                    'nombre': r.nombre_regla,
                    'logica': logica_humanizada, 
                    'efecto': efecto,
                    'mensaje': r.payload_json.get('mensaje_alerta', '')
                })
            tabla.rows = filas
            tabla.visible = bool(filas)
            vacio.visible = not filas

    def guardar():
        if not regla['nombre'] or not regla['trigger']:
            ui.notify('Faltan datos obligatorios', type='warning')
            return

        with get_db_session() as db:
            var_obj = db.query(MetaVariable).filter_by(nombre_interno=regla['trigger']).first()
            
            nueva = MetaRegla(
                nombre_regla=regla['nombre'],
                variable_id=var_obj.id,
                operador=regla['op'],
                valor_comparacion_json=regla['val'],
                tipo_consecuencia=regla['cons'],
                payload_json={
                    'mensaje_alerta': regla['msg'],
                    'formatos_permitidos': regla['ext'] if regla['cons'] == 'FILE_REQ' else [],
                    'rol_destino': regla['msg'] if regla['cons'] == 'NOTIFY_ROLE' else None,
                    'dias_ajuste': int(regla['dias_ajuste'] or 0) if regla['cons'] == 'DATE_CHANGE' else None
                }
            )
            db.add(nueva)
            db.commit()
            
        ui.notify('Regla Guardada', type='positive')
        dialogo.close()
        cargar_tabla()
        # Reset simple del formulario
        regla['nombre'] = ''
        regla['val'] = ''

    # --- 3. INTERFAZ ---
    ui.label('🧠 Motor de Causalidad').classes('text-2xl font-bold mb-4 text-slate-800')

    # Tabla Mejorada con HTML habilitado para negritas
    cols = [
        {'name': 'nombre', 'label': 'Nombre', 'field': 'nombre', 'align': 'left'},
        {'name': 'logica', 'label': 'Condición (Causa)', 'field': 'logica', 'align': 'left', 'html': True}, # HTML True permite negritas
        {'name': 'efecto', 'label': 'Consecuencia', 'field': 'efecto', 'align': 'left'},
        {'name': 'mensaje', 'label': 'Mensaje Usuario', 'field': 'mensaje', 'align': 'left'},
        {'name': 'acciones', 'label': 'Acción', 'field': 'acciones', 'align': 'center'}
    ]
    
    tabla = ui.table(columns=cols, rows=[]).classes('w-full shadow-sm border rounded')
    
    # Slot para botón de borrar en cada fila
    tabla.add_slot('body-cell-acciones', '''
        <q-td :props="props">
            <q-btn icon="delete" color="negative" flat dense @click="$parent.$emit('borrar', props.row.id)" />
        </q-td>
    ''')
    tabla.on('borrar', lambda e: borrar_regla(e.args))

    # Estado vacío: explica qué es una regla antes de que exista la primera.
    vacio = estado_vacio(
        'rule',
        'Aún no tienes reglas de negocio',
        'Una regla dice: "SI un dato cumple una condición, ENTONCES ocurre algo" '
        '— pedir un archivo, notificar a alguien, bloquear el proceso o ajustar la '
        'fecha límite. Son el cerebro que automatiza tus controles.',
        'Crear primera regla',
        lambda: dialogo.open(),
    )

    cargar_tabla()

    # Botón Flotante
    ui.button(icon='add', on_click=lambda: dialogo.open()) \
        .props('fab').classes('absolute bottom-4 right-4 bg-orange-600 text-white shadow-lg')

    # Modal de Creación
    with ui.dialog() as dialogo, ui.card().classes('min-w-[700px]'):
        ui.label('Nueva Regla Lógica').classes('text-xl font-bold mb-4 text-slate-700')
        
        ui.input('Nombre de la Regla (Ej: Corte día 7)').bind_value(regla, 'nombre').classes('w-full mb-4')
        
        # Bloque SI (Condición)
        with ui.row().classes('items-center gap-0 no-wrap'):
            ui.label('SI (Condición)').classes('text-xs font-bold text-gray-500 uppercase')
            ayuda('La condición que se evalúa sobre un dato capturado. Ejemplo: '
                  'SI [monto] es MAYOR QUE 1000. Elige la variable, el operador de '
                  'comparación y el valor con el que se compara.')
        with ui.row().classes('w-full gap-2 items-center bg-slate-50 p-3 rounded border mb-4'):
            ui.icon('quiz').classes('text-blue-500 text-2xl')
            ui.select(opciones_vars, label='Variable').bind_value(regla, 'trigger').classes('flex-grow')
            ui.select(['EQUALS', 'GREATER_THAN', 'LESS_THAN', 'GREATER_THAN_OR_EQUAL', 'LESS_THAN_OR_EQUAL', 'CONTAINS', 'IN_LIST'], label='Operador').bind_value(regla, 'op').classes('w-40')
            ui.input('Valor').bind_value(regla, 'val').classes('w-32')

        # Bloque ENTONCES (Consecuencia)
        with ui.row().classes('items-center gap-0 no-wrap'):
            ui.label('ENTONCES (Efecto)').classes('text-xs font-bold text-gray-500 uppercase')
            ayuda('Qué ocurre si la condición se cumple. '
                  'Exigir Archivo: pide un documento en los formatos que elijas. '
                  'Notificar Rol: avisa a un rol del sistema. '
                  'Bloquear: impide que el proceso continúe. '
                  'Cambiar Fecha: ajusta la fecha límite en +/- días.')
        with ui.row().classes('w-full gap-2 items-center bg-orange-50 p-3 rounded border mb-4'):
            ui.icon('bolt').classes('text-orange-500 text-2xl')
            efecto_sel = ui.select(
                {'FILE_REQ': 'Exigir Archivo', 'NOTIFY_ROLE': 'Notificar Rol', 'BLOCK': 'Bloquear', 'DATE_CHANGE': 'Cambiar Fecha'},
                label='Acción'
            ).bind_value(regla, 'cons').classes('flex-grow')

        # Detalles dinámicos
        with ui.column().classes('w-full pl-4 border-l-4 border-orange-200'):
            ui.input('Mensaje de Alerta al Usuario').bind_value(regla, 'msg').classes('w-full')
            
            # Mostrar solo si es Archivo
            with ui.row().bind_visibility_from(efecto_sel, 'value', lambda v: v == 'FILE_REQ').classes('items-center w-full'):
                ui.label('Formatos:').classes('mr-2 text-sm')
                ui.select(['pdf', 'xlsx', 'zip', 'jpg', 'png'], multiple=True, label='Seleccionar') \
                    .bind_value(regla, 'ext').classes('flex-grow')

            # Mostrar solo si es Cambiar Fecha
            with ui.row().bind_visibility_from(efecto_sel, 'value', lambda v: v == 'DATE_CHANGE').classes('items-center w-full'):
                ui.label('Ajuste:').classes('mr-2 text-sm')
                ui.number('Días de ajuste (+/-)', format='%.0f').bind_value(regla, 'dias_ajuste').classes('flex-grow')

        # Footer
        with ui.row().classes('w-full justify-end mt-6'):
            ui.button('Cancelar', on_click=dialogo.close).props('flat color=grey')
            ui.button('Guardar Regla', on_click=guardar).classes('bg-green-600 text-white px-6')