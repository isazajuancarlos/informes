from nicegui import ui, app
from app.services.backup_service import BackupService
import os

def page_backup_manager():
    service = BackupService()
    
    # --- LÓGICA ---
    def refrescar_tabla():
        rows = service.listar_copias()
        tabla.rows = rows

    def crear_manual():
        ui.notify('Iniciando copia de seguridad...', type='info')
        ok, msg = service.crear_copia(automatico=False)
        if ok:
            ui.notify(f'Copia creada: {msg}', type='positive')
            refrescar_tabla()
        else:
            ui.notify(f'Error: {msg}', type='negative', multi_line=True)

    def confirmar_restauracion(fila):
        with ui.dialog() as d, ui.card().classes('border-l-4 border-red-600'):
            ui.label('⚠️ PELIGRO: RESTAURACIÓN DE SISTEMA').classes('text-xl font-bold text-red-600')
            ui.label(f'Vas a restaurar el archivo "{fila["nombre"]}".').classes('font-bold')
            ui.label('Esto BORRARÁ todos los datos actuales y los reemplazará por esta copia.').classes('text-sm')
            ui.label('Esta acción es irreversible.').classes('text-sm font-bold')
            
            def proceder():
                d.close()
                ui.notify('⏳ Restaurando... Por favor espere.', type='warning', timeout=5000)
                ok, msg = service.restaurar_copia(fila['nombre'])
                if ok:
                    ui.notify('✅ Sistema restaurado exitosamente', type='positive', close_button=True)
                else:
                    ui.notify(f'❌ Falló la restauración: {msg}', type='negative', multi_line=True, close_button=True)

            with ui.row().classes('w-full justify-end mt-4'):
                ui.button('Cancelar', on_click=d.close).props('flat')
                ui.button('SÍ, RESTAURAR', color='red', on_click=proceder)
        d.open()

    def descargar_archivo(fila):
        ui.download(fila['path'])

    def eliminar_archivo(fila):
        if service.eliminar_copia(fila['nombre']):
            ui.notify('Archivo eliminado', type='info')
            refrescar_tabla()
        else:
            ui.notify('No se pudo eliminar', type='negative')

    # --- UI ---
    ui.label('Copias de Seguridad y Restauración').classes('text-2xl font-bold text-slate-800 mb-4')

    with ui.row().classes('w-full items-start gap-6'):
        
        # PANEL IZQUIERDO: ACCIONES
        with ui.card().classes('w-1/4 p-4'):
            ui.label('Acciones').classes('text-lg font-bold text-blue-800 mb-2')
            ui.label('Genere una copia manual del estado actual del sistema.').classes('text-xs text-gray-500 mb-4')
            
            ui.button('Generar Nueva Copia', on_click=crear_manual, icon='save') \
                .classes('w-full bg-blue-700 text-white mb-4')
            
            ui.separator()
            
            ui.label('Configuración Automática').classes('font-bold text-sm mt-2')
            # Usamos el almacenamiento persistente de NiceGUI para guardar la config
            chk_auto = ui.checkbox('Habilitar Respaldo Diario (00:00 AM)') \
                .bind_value(app.storage.general, 'backup_auto_activo')
            
            ui.label('El sistema guardará una copia automáticamente cada noche si está activo.').classes('text-xs text-gray-400')

        # PANEL DERECHO: LISTA
        with ui.column().classes('w-3/4'):
            cols = [
                {'name': 'nombre', 'label': 'Archivo', 'field': 'nombre', 'align': 'left'},
                {'name': 'fecha', 'label': 'Fecha Creación', 'field': 'fecha', 'align': 'left', 'sortable': True},
                {'name': 'tipo', 'label': 'Tipo', 'field': 'tipo', 'align': 'center'},
                {'name': 'tamano', 'label': 'Tamaño', 'field': 'tamano', 'align': 'right'},
                {'name': 'acciones', 'label': 'Operaciones', 'field': 'acciones', 'align': 'center'},
            ]
            
            tabla = ui.table(columns=cols, rows=[], pagination=10).classes('w-full shadow-sm border rounded')
            
            # Badges visuales
            tabla.add_slot('body-cell-tipo', '''
                <q-td :props="props">
                    <q-badge :color="props.row.tipo == 'AUTOMÁTICA' ? 'purple' : 'blue'">
                        {{ props.row.tipo }}
                    </q-badge>
                </q-td>
            ''')
            
            # Botones de acción por fila
            tabla.add_slot('body-cell-acciones', '''
                <q-td :props="props">
                    <q-btn icon="history" color="red" size="sm" flat label="Restaurar"
                           @click="$parent.$emit('restore', props.row)">
                        <q-tooltip>Peligroso: Sobrescribe datos</q-tooltip>
                    </q-btn>
                    <q-btn icon="download" color="green" round flat size="sm" 
                           @click="$parent.$emit('download', props.row)" />
                    <q-btn icon="delete" color="grey" round flat size="sm" 
                           @click="$parent.$emit('delete', props.row)" />
                </q-td>
            ''')
            
            tabla.on('restore', lambda e: confirmar_restauracion(e.args))
            tabla.on('download', lambda e: descargar_archivo(e.args))
            tabla.on('delete', lambda e: eliminar_archivo(e.args))
            
            refrescar_tabla()