"""
Simulador de Reglas de Negocio — usa el MotorCausalidad real.
"""
from nicegui import ui, app
from app.core.database import get_db_session
from app.models.meta_tables import Informe, ConfiguracionInformeVariable, MetaVariable, MetaRegla
import os
import time
from datetime import datetime


def page_simulator():
    estado = {
        'informe_id': None,
        'variables_config': [],   # List of (nombre_interno, tipo, valor_parametro)
        'valores_usuario': {},    # Dict filled by form widgets
        'reglas_orm': []          # MetaRegla ORM objects loaded for the informe
    }

    resultado_container = ui.column().classes('w-full gap-2')
    form_container = ui.column().classes('w-full gap-3 my-4')

    def cargar_configuracion():
        if not estado['informe_id']:
            return

        inf_id = int(estado['informe_id'])
        with get_db_session() as db:
            # Load variable configurations
            configs = db.query(ConfiguracionInformeVariable).filter_by(informe_id=inf_id).all()
            estado['variables_config'] = [
                (cfg.variable.nombre_interno, cfg.variable.tipo_dato,
                 cfg.variable.etiqueta_visual, cfg.valor_parametro)
                for cfg in configs
            ]

            # Load rules for this informe via its variables
            var_ids = [cfg.variable_id for cfg in configs]
            reglas = db.query(MetaRegla).filter(MetaRegla.variable_id.in_(var_ids)).all() if var_ids else []
            # Store serializable data (not ORM objects)
            estado['reglas_orm'] = [
                {
                    'nombre_regla': r.nombre_regla,
                    'variable_trigger': r.variable.nombre_interno,
                    'operador': r.operador,
                    'valor_comparacion': r.valor_comparacion_json,
                    'tipo_consecuencia': r.tipo_consecuencia,
                    'payload': r.payload_json or {}
                }
                for r in reglas
            ]

        estado['valores_usuario'] = {}
        _render_form()

    def _render_form():
        form_container.clear()
        resultado_container.clear()

        if not estado['variables_config']:
            with form_container:
                ui.label('Este informe no tiene variables configuradas.').classes('text-slate-400')
            return

        with form_container:
            ui.label('Valores de entrada').classes('text-lg font-bold')
            for nombre_interno, tipo_dato, etiqueta, valor_param in estado['variables_config']:
                estado['valores_usuario'].setdefault(nombre_interno, '')

                if tipo_dato in ('NUMERO', 'MONEDA'):
                    ui.number(label=etiqueta, format='%.2f').bind_value(
                        estado['valores_usuario'], nombre_interno
                    ).classes('w-full')
                elif tipo_dato == 'BOOLEANO':
                    ui.checkbox(text=etiqueta).bind_value(
                        estado['valores_usuario'], nombre_interno
                    )
                elif tipo_dato == 'FECHA':
                    ui.input(label=etiqueta).props('type=date').bind_value(
                        estado['valores_usuario'], nombre_interno
                    ).classes('w-full')
                elif tipo_dato == 'HORA_MILITAR':
                    ui.input(label=etiqueta, placeholder='HHMM').bind_value(
                        estado['valores_usuario'], nombre_interno
                    ).classes('w-full')
                else:
                    ui.input(label=etiqueta, value=valor_param or '').bind_value(
                        estado['valores_usuario'], nombre_interno
                    ).classes('w-full')

            ui.button('Ejecutar simulación', icon='play_arrow', on_click=ejecutar_simulacion) \
                .classes('bg-blue-600 text-white mt-2')

    def ejecutar_simulacion():
        from app.services.causality_engine import MotorCausalidad
        from app.schemas.dynamic_core import MetaReglaCreate, ConsecuenciaPayload, OperadorLogicoEnum, TipoConsecuenciaEnum

        resultado_container.clear()

        reglas_raw = estado['reglas_orm']
        if not reglas_raw:
            with resultado_container:
                ui.label('Este informe no tiene reglas configuradas.').classes('text-slate-400 italic')
            return

        # Convert raw dicts to Pydantic MetaReglaCreate objects
        reglas_pydantic = []
        for r in reglas_raw:
            try:
                payload = ConsecuenciaPayload(**(r['payload'] or {}))
                regla = MetaReglaCreate(
                    nombre_regla=r['nombre_regla'],
                    variable_trigger=r['variable_trigger'],
                    operador=OperadorLogicoEnum(r['operador']),
                    valor_comparacion=r['valor_comparacion'],
                    tipo_consecuencia=TipoConsecuenciaEnum(r['tipo_consecuencia']),
                    payload=payload
                )
                reglas_pydantic.append(regla)
            except Exception as e:
                print(f"[Simulator] Error converting rule: {e}")

        motor = MotorCausalidad()
        plan = motor.evaluar_impacto(estado['valores_usuario'], reglas_pydantic)

        with resultado_container:
            ui.label('Resultados de la simulación').classes('text-xl font-bold mt-4')

            if plan['proceso_bloqueado']:
                ui.chip('PROCESO BLOQUEADO', color='red', icon='block').classes('text-white font-bold')

            if plan['alertas_generadas']:
                ui.label('Alertas generadas').classes('font-bold text-orange-600 mt-2')
                for alerta in plan['alertas_generadas']:
                    with ui.row().classes('items-center gap-2'):
                        ui.icon('warning', color='orange')
                        ui.label(alerta)

            if plan['archivos_requeridos']:
                ui.label('Archivos requeridos').classes('font-bold text-blue-600 mt-2')
                for fmt in plan['archivos_requeridos']:
                    with ui.row().classes('items-center gap-2'):
                        ui.icon('attach_file', color='blue')
                        ui.label(fmt)

            if plan['roles_a_notificar']:
                ui.label('Roles a notificar').classes('font-bold text-purple-600 mt-2')
                for rol in plan['roles_a_notificar']:
                    ui.chip(rol, color='purple', icon='notifications').classes('text-white')

            if plan['ajuste_fecha_dias']:
                ui.label(
                    f"Ajuste de fecha: {plan['ajuste_fecha_dias']:+d} días"
                ).classes('font-bold text-teal-600 mt-2')

            if not any([plan['alertas_generadas'], plan['archivos_requeridos'],
                        plan['roles_a_notificar'], plan['ajuste_fecha_dias'],
                        plan['proceso_bloqueado']]):
                ui.label('Sin consecuencias. Todas las reglas pasaron sin activarse.').classes('text-green-600')

    # --- UI Layout ---
    with ui.column().classes('w-full p-4 max-w-2xl'):
        ui.label('Simulador de Reglas').classes('text-2xl font-bold')
        ui.label('Selecciona un informe, completa los valores y ejecuta para ver las reglas aplicadas.') \
            .classes('text-slate-500 mb-4')

        with get_db_session() as db:
            informes = db.query(Informe).filter_by(activo=True).order_by(Informe.nombre).all()
            opts = {str(i.id): i.nombre for i in informes}

        ui.select(
            opts,
            label='Informe',
            on_change=cargar_configuracion,
            with_input=True
        ).bind_value(estado, 'informe_id').classes('w-full')

    form_container
    resultado_container
