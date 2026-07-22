from nicegui import ui
from app.core.database import get_db_session
from app.models.meta_tables import (MetaVariable, Informe, MetaRegla,
                                    AMBITO_CONTENIDO, AMBITO_FICHA, AMBITOS)
from app.ui.ui_helpers import estado_vacio, ayuda

#: Cómo se le dice a cada ámbito en pantalla. La clave es del motor; esto es
#: para personas, y describe QUÉ HACE el campo, no cómo se llama por dentro.
ETIQUETA_AMBITO = {
    AMBITO_FICHA: 'Describe el informe (ficha)',
    AMBITO_CONTENIDO: 'Es un dato de la entrega',
}


def page_variables():
    # Estado para controlar si estamos editando o creando
    estado = {'id_edicion': None}

    # --- LÓGICA DE DATOS ---
    def cargar():
        with get_db_session() as db:
            variables = db.query(MetaVariable).order_by(MetaVariable.id).all()
            filas = []
            for v in variables:
                filas.append({
                    "id": v.id,
                    "nombre_interno": v.nombre_interno,
                    "etiqueta_visual": v.etiqueta_visual,
                    "tipo_dato": v.tipo_dato,
                    "ambito": v.ambito or AMBITO_CONTENIDO,
                    "ambito_txt": ETIQUETA_AMBITO.get(v.ambito or AMBITO_CONTENIDO,
                                                      v.ambito or ""),
                    "opciones": v.opciones or "",
                    "es_obligatorio": v.es_obligatorio,
                    "obligatorio_txt": "Sí" if v.es_obligatorio else "No"
                })
            tabla.rows = filas
            # Estado vacío: si no hay variables, guía en vez de tabla vacía.
            tabla.visible = bool(filas)
            vacio.visible = not filas

    def limpiar_form():
        nombre.value = ''
        etiqueta.value = ''
        tipo.value = 'TEXTO'
        ambito.value = AMBITO_FICHA
        opciones.value = ''
        obligatorio.value = True
        estado['id_edicion'] = None
        label_titulo.text = 'Nuevo campo'

    def abrir_modal_edicion(fila):
        estado['id_edicion'] = fila['id']
        nombre.value = fila['nombre_interno']
        etiqueta.value = fila['etiqueta_visual']
        tipo.value = fila['tipo_dato']
        ambito.value = fila.get('ambito') or AMBITO_CONTENIDO
        opciones.value = fila.get('opciones') or ''
        obligatorio.value = fila['es_obligatorio']

        label_titulo.text = f'Editar campo (ID: {fila["id"]})'
        dialogo.open()

    def validar_dependencias(db, id_var):
        # 1. Revisar REGLAS
        regla_uso = db.query(MetaRegla).filter_by(variable_id=id_var).first()
        if regla_uso:
            return False, f"No se puede eliminar: Se usa en la regla '{regla_uso.nombre_regla}'"

        # 2. Revisar INFORMES (Búsqueda en JSON)
        informes = db.query(Informe).all()
        for inf in informes:
            if inf.variables_ids and id_var in inf.variables_ids:
                return False, f"No se puede eliminar: Se usa en el informe '{inf.id}'"

        return True, "OK"

    def eliminar(id_var):
        with get_db_session() as db:
            puede_borrar, mensaje = validar_dependencias(db, id_var)
            
            if not puede_borrar:
                ui.notify(mensaje, type='negative', close_button=True, multi_line=True)
                return

            var_obj = db.query(MetaVariable).get(id_var)
            if var_obj:
                db.delete(var_obj)
                db.commit()
                ui.notify('Variable eliminada correctamente', type='positive')
                cargar()
            else:
                ui.notify('La variable no existe', type='warning')

    def guardar():
        if not nombre.value or not etiqueta.value:
            ui.notify('Complete campos obligatorios', type='warning')
            return
        
        with get_db_session() as db:
            # --- MODO EDICIÓN ---
            if estado['id_edicion']:
                var_obj = db.query(MetaVariable).get(estado['id_edicion'])
                if var_obj:
                    var_obj.nombre_interno = nombre.value
                    var_obj.etiqueta_visual = etiqueta.value
                    var_obj.tipo_dato = tipo.value
                    var_obj.ambito = ambito.value
                    var_obj.opciones = opciones.value or None
                    var_obj.es_obligatorio = obligatorio.value
                    db.commit()
                    ui.notify('Variable actualizada', type='positive')
            
            # --- MODO CREACIÓN ---
            else:
                existe = db.query(MetaVariable).filter_by(nombre_interno=nombre.value).first()
                if existe:
                    ui.notify(f'El ID "{nombre.value}" ya existe', type='negative')
                    return

                # CORRECCIÓN: Se eliminó 'opciones_json=[]' que causaba el error
                db.add(MetaVariable(
                    nombre_interno=nombre.value,
                    etiqueta_visual=etiqueta.value,
                    tipo_dato=tipo.value,
                    ambito=ambito.value,
                    opciones=opciones.value or None,
                    es_obligatorio=obligatorio.value
                ))
                db.commit()
                ui.notify('Variable creada', type='positive')
        
        cargar()
        dialogo.close()
        limpiar_form()

    # --- INTERFAZ UI ---
    ui.label('Campos de los informes').classes('text-2xl font-bold mb-4 text-slate-800')
    
    cols = [
        {'name': 'nombre_interno', 'label': 'ID Interno', 'field': 'nombre_interno', 'align': 'left'},
        {'name': 'etiqueta_visual', 'label': 'Pregunta Visual', 'field': 'etiqueta_visual', 'align': 'left'},
        {'name': 'ambito_txt', 'label': 'Para qué sirve', 'field': 'ambito_txt', 'align': 'left'},
        {'name': 'tipo_dato', 'label': 'Tipo', 'field': 'tipo_dato', 'align': 'left'},
        {'name': 'obligatorio_txt', 'label': 'Obligatorio', 'field': 'obligatorio_txt', 'align': 'center'},
        {'name': 'acciones', 'label': 'Acciones', 'field': 'acciones', 'align': 'center'},
    ]
    
    tabla = ui.table(columns=cols, rows=[]).classes('w-full shadow-sm border rounded')
    
    tabla.add_slot('body-cell-acciones', '''
        <q-td :props="props">
            <q-btn icon="edit" color="primary" flat dense size="sm" @click="$parent.$emit('editar', props.row)" />
            <q-btn icon="delete" color="negative" flat dense size="sm" @click="$parent.$emit('borrar', props.row)" />
        </q-td>
    ''')

    tabla.on('editar', lambda e: abrir_modal_edicion(e.args))
    
    def confirmar_borrado(fila):
        with ui.dialog() as d, ui.card():
            ui.label(f'¿Estás seguro de eliminar "{fila["nombre_interno"]}"?')
            with ui.row():
                ui.button('Cancelar', on_click=d.close).props('flat')
                ui.button('Eliminar', color='red', on_click=lambda: [eliminar(fila['id']), d.close()])
        d.open()

    tabla.on('borrar', lambda e: confirmar_borrado(e.args))

    # Panel de estado vacío (se muestra solo cuando no hay variables).
    vacio = estado_vacio(
        'category',
        'Aún no tienes campos',
        'Los campos los define usted, no el programa. Unos describen el INFORME '
        '—subsistema, aplicativo de origen, la norma que lo exige— y se contestan '
        'al crearlo; otros son datos que la ENTREGA lleva dentro y se validan al '
        'entregarla. Ninguna lista viene impuesta.',
        'Crear el primer campo',
        lambda: [limpiar_form(), dialogo.open()],
    )

    cargar()

    # --- MODAL FORMULARIO ---
    with ui.dialog() as dialogo, ui.card().classes('min-w-[400px]'):
        label_titulo = ui.label('Nueva Variable').classes('text-lg font-bold mb-4')
        
        with ui.row().classes('w-full items-center gap-0 no-wrap'):
            ui.label('ID Interno').classes('text-xs text-slate-500')
            ayuda('Nombre técnico sin espacios ni acentos (ej: fecha_corte). '
                  'Es el identificador interno; no lo verá el usuario final.')
        nombre = ui.input('ID Interno (ej: fecha_corte)').classes('w-full')
        with ui.row().classes('w-full items-center gap-0 no-wrap'):
            ui.label('Pregunta Visual').classes('text-xs text-slate-500')
            ayuda('El texto que verá el operador al capturar el dato '
                  '(ej: "Fecha de corte del período").')
        etiqueta = ui.input('Pregunta Visual').classes('w-full')
        with ui.row().classes('w-full items-center gap-0 no-wrap'):
            ui.label('Tipo de dato').classes('text-xs text-slate-500')
            ayuda('Define cómo se captura y valida el dato. HORA_MILITAR: hora en '
                  'formato 24h (14:30). MONEDA: valor monetario con formato. '
                  'TEXTO_ALFANUM: texto y números sin caracteres especiales. '
                  'LISTA: opciones predefinidas.')
        tipo = ui.select(['TEXTO', 'NUMERO', 'FECHA', 'BOOLEANO', 'LISTA', 'HORA_MILITAR', 'MONEDA', 'TEXTO_ALFANUM'], value='TEXTO', label='Tipo').classes('w-full')
        with ui.row().classes('w-full items-center gap-0 no-wrap mt-2'):
            ui.label('Para qué sirve').classes('text-xs text-slate-500')
            ayuda('FICHA: describe el INFORME —subsistema, aplicativo de origen, '
                  'cargo que autoriza, norma que lo exige—. Se contesta al crear '
                  'el informe.\n\n'
                  'CONTENIDO: es un dato que la ENTREGA lleva dentro —monto, hora '
                  'de corte, número de radicado— y que se valida cuando alguien '
                  'entrega.\n\n'
                  'Son preguntas distintas sobre el mismo informe, y por eso se '
                  'muestran en sitios distintos.')
        ambito = ui.select({a: ETIQUETA_AMBITO[a] for a in AMBITOS},
                           value=AMBITO_FICHA, label='Para qué sirve') \
            .props('outlined').classes('w-full').mark('ambito-campo')
        opciones = ui.textarea(
            'Opciones (una por línea)',
            placeholder='Vigilancia\nAseguramiento\nMorbilidad').classes('w-full')
        opciones.bind_visibility_from(tipo, 'value', backward=lambda v: v == 'LISTA')
        obligatorio = ui.switch('Obligatorio', value=True)
        
        with ui.row().classes('w-full justify-end mt-4'):
            ui.button('Cancelar', on_click=dialogo.close).props('flat color=grey')
            ui.button('Guardar', on_click=guardar).classes('bg-blue-600 text-white')

    ui.button(icon='add', on_click=lambda: [limpiar_form(), dialogo.open()]) \
        .props('fab').classes('absolute bottom-4 right-4 bg-blue-600 shadow-lg text-white')