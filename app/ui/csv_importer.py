from nicegui import ui
from app.services.csv_logic import CsvLogic

def page_csv_importer():
    
    # --- LÓGICA ---
    def descargar_plantilla():
        csv_content = CsvLogic.generar_plantilla()
        ui.download(csv_content.encode('utf-8'), 'plantilla_carga_masiva.csv')

    async def procesar_archivo(e):
        ui.notify('Procesando archivo...', type='info')
        
        # Leer contenido
        contenido = e.content.read()
        
        # Procesar en el backend
        resultado = CsvLogic.procesar_archivo(contenido)
        
        if 'error_critico' in resultado:
            ui.notify(resultado['error_critico'], type='negative', close_button=True)
            return

        # Mostrar Resultados
        contenedor_resultados.clear()
        with contenedor_resultados:
            ui.label(f"Resumen: {resultado['total']} procesados | {resultado['exitos']} exitosos | {resultado['errores']} fallidos").classes('text-lg font-bold mb-4')
            
            # Tabla de Detalles
            cols = [
                {'name': 'fila', 'label': 'Fila', 'field': 'fila', 'align': 'center'},
                {'name': 'estado', 'label': 'Estado', 'field': 'estado', 'align': 'center'},
                {'name': 'msg', 'label': 'Detalle', 'field': 'msg', 'align': 'left'},
            ]
            
            rows = resultado['detalles']
            tabla = ui.table(columns=cols, rows=rows).classes('w-full shadow-sm')
            
            # Colores según estado
            tabla.add_slot('body-cell-estado', '''
                <q-td :props="props">
                    <q-badge :color="props.row.estado.includes('ERROR') ? 'red' : 'green'">
                        {{ props.row.estado }}
                    </q-badge>
                </q-td>
            ''')
            
        if resultado['errores'] == 0:
            ui.notify('Carga masiva completada con éxito', type='positive')
        else:
            ui.notify('La carga finalizó con errores. Revise el reporte.', type='warning')

    # --- UI ---
    ui.label('Carga Masiva de Informes (CSV)').classes('text-2xl font-bold text-slate-800 mb-4')

    with ui.card().classes('w-full p-6'):
        # PASO 1
        ui.label('1. Descargar Plantilla').classes('text-lg font-bold text-blue-800')
        ui.label('Descargue el archivo CSV con la estructura actual de variables del sistema.').classes('text-sm text-gray-500')
        ui.button('Descargar Plantilla Actualizada', icon='download', on_click=descargar_plantilla).classes('mt-2 bg-blue-600 text-white')
        
        ui.separator().classes('my-6')

        # PASO 2
        ui.label('2. Subir Archivo Diligenciado').classes('text-lg font-bold text-green-800')
        ui.label('El sistema validará tipos de datos, fechas y correos antes de guardar.').classes('text-sm text-gray-500 mb-2')
        
        ui.upload(
            label='Arrastre su archivo CSV aquí',
            auto_upload=True,
            on_upload=procesar_archivo,
            max_files=1
        ).props('accept=".csv"').classes('w-full border-2 border-dashed border-gray-300 p-4 rounded-lg')

    # ZONA DE RESULTADOS
    ui.separator().classes('my-6')
    contenedor_resultados = ui.column().classes('w-full')