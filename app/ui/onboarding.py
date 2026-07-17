"""
Onboarding — guía de "Primeros pasos" para un administrador nuevo.

Fase 1 del asistente amigable (sin LLM): detecta qué tiene configurado el admin
y lo lleva al siguiente paso, en vez de recibirlo con un tablero de ceros.

El panel se muestra mientras la configuración esté incompleta; cuando todo está
listo, se colapsa a una felicitación discreta que el admin puede ocultar.
"""
from nicegui import ui, app
from app.core.database import get_db_session
from app.models.meta_tables import MetaVariable, MetaRegla, Informe, Usuario


def _estado_configuracion() -> dict:
    """Cuenta las piezas mínimas que un sistema útil necesita tener."""
    with get_db_session() as db:
        return {
            "variables": db.query(MetaVariable).count(),
            "reglas": db.query(MetaRegla).count(),
            "informes": db.query(Informe).count(),
            "operadores": db.query(Usuario).filter(Usuario.es_admin == False).count(),  # noqa: E712
        }


# (título, por qué importa en lenguaje llano, clave de conteo, ruta, icono)
_PASOS = [
    ("Crea tu primera variable",
     "Las variables son los datos que capturan tus informes: fechas, montos, responsables. Son la materia prima de todo lo demás.",
     "variables", "/variables", "add_chart"),
    ("Define una regla de negocio",
     "Las reglas deciden qué ocurre según los valores capturados: enviar una alerta, bloquear un proceso o ajustar una fecha límite.",
     "reglas", "/reglas", "rule"),
    ("Registra un informe",
     "Un informe es la obligación que el sistema programa en el tiempo y audita: cada cuánto vence, quién responde y si se cumplió.",
     "informes", "/gestor-informes", "description"),
    ("Invita a un operador",
     "Los operadores registran las entregas del día a día; tú supervisas. Sin ellos, todo el trabajo recae en el administrador.",
     "operadores", "/gestor-usuarios", "group_add"),
]

_OCULTAR_KEY = "onboarding_oculto"


def panel_primeros_pasos():
    """Renderiza la guía de primeros pasos en el tablero. No hace nada si el
    admin ya la ocultó tras completarla."""
    estado = _estado_configuracion()
    pasos = [(titulo, por_que, estado.get(clave, 0) > 0, ruta, icono)
             for (titulo, por_que, clave, ruta, icono) in _PASOS]
    hechos = sum(1 for p in pasos if p[2])
    total = len(pasos)
    completo = hechos == total

    # Si ya está todo y el admin ocultó el panel, no reaparece.
    if completo and app.storage.user.get(_OCULTAR_KEY):
        return

    color_borde = "border-green-500" if completo else "border-indigo-500"
    with ui.card().classes(f"w-full p-6 mb-8 border-l-4 {color_borde} shadow-sm"):
        with ui.row().classes("w-full items-center justify-between mb-1 flex-nowrap"):
            with ui.row().classes("items-center gap-2"):
                ui.icon("rocket_launch" if not completo else "verified").classes(
                    "text-2xl " + ("text-indigo-500" if not completo else "text-green-600"))
                ui.label("Primeros pasos" if not completo else "¡Configuración completa!").classes(
                    "text-lg font-bold text-slate-800")
            ui.label(f"{hechos} de {total}").classes("text-sm font-semibold text-slate-400")

        if completo:
            ui.label("Tu sistema ya tiene lo esencial. Puedes ocultar esta guía.").classes(
                "text-sm text-slate-500 mb-2")
            ui.button("Ocultar guía", icon="check",
                      on_click=_ocultar).props("flat dense color=green")
            return

        ui.label("Configura tu sistema en cuatro pasos. Empieza por donde quieras; "
                 "cada paso te lleva a la pantalla correspondiente.").classes(
            "text-sm text-slate-500 mb-4")

        # Barra de progreso simple.
        ui.linear_progress(value=hechos / total, show_value=False).props(
            "color=indigo").classes("mb-4")

        for titulo, por_que, hecho, ruta, icono in pasos:
            with ui.row().classes("w-full items-center gap-4 py-2 flex-nowrap border-b border-slate-100"):
                ui.icon("check_circle" if hecho else "radio_button_unchecked").classes(
                    "text-2xl " + ("text-green-500" if hecho else "text-slate-300"))
                ui.icon(icono).classes("text-xl text-slate-400")
                with ui.column().classes("gap-0 flex-1 min-w-0"):
                    ui.label(titulo).classes(
                        "font-semibold " + ("text-slate-400 line-through" if hecho else "text-slate-700"))
                    ui.label(por_que).classes("text-xs text-slate-500")
                if hecho:
                    ui.label("Hecho").classes("text-xs font-semibold text-green-600 flex-none")
                else:
                    ui.button("Ir", icon="arrow_forward",
                              on_click=lambda r=ruta: ui.navigate.to(r)).props(
                        "flat dense color=indigo").classes("flex-none")


def _ocultar():
    app.storage.user[_OCULTAR_KEY] = True
    ui.navigate.reload()
