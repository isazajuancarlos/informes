"""
Helpers de UI para la Fase 1 del asistente amigable (sin LLM):

- estado_vacio(): reemplaza el árido "No data" por un panel que explica qué es
  esa lista y ofrece crear el primer elemento.
- ayuda(): un icono de ayuda con tooltip, para desdensificar los campos difíciles
  (frecuencias, cortes, retroactividad) sin saturar la pantalla.

Son puramente de presentación: funcionan igual en despliegues online y offline.
"""
from nicegui import ui


def estado_vacio(icono: str, titulo: str, mensaje: str,
                 texto_boton: str = None, on_click=None):
    """Panel amable para cuando una lista está vacía. Devuelve el contenedor,
    para poder mostrarlo/ocultarlo según haya datos o no::

        vacio = estado_vacio("inventory_2", "Aún no tienes variables",
                             "Son los datos que capturan tus informes.",
                             "Crear la primera", abrir_modal)
        vacio.visible = False        # se activa desde cargar() si no hay filas
    """
    with ui.column().classes(
            "w-full items-center justify-center py-16 gap-3 "
            "border border-dashed border-slate-200 rounded-lg bg-slate-50/50") as cont:
        ui.icon(icono).classes("text-6xl text-slate-300")
        ui.label(titulo).classes("text-lg font-semibold text-slate-600")
        ui.label(mensaje).classes("text-sm text-slate-400 text-center max-w-md px-4")
        if texto_boton and on_click is not None:
            ui.button(texto_boton, icon="add", on_click=on_click).classes(
                "bg-blue-600 text-white mt-2")
    return cont


def ayuda(texto: str):
    """Icono de ayuda con tooltip contextual, para poner junto a un campo o una
    etiqueta densa. El texto aparece al pasar el cursor."""
    ic = ui.icon("help_outline").classes("text-slate-400 text-base cursor-help ml-1")
    with ic:
        ui.tooltip(texto).classes("max-w-xs text-sm bg-slate-800")
    return ic


def etiqueta_con_ayuda(texto_etiqueta: str, texto_ayuda: str,
                       clases: str = "font-semibold text-slate-700"):
    """Una etiqueta seguida de un icono de ayuda, en una fila. Útil encima de un
    campo de formulario que necesita explicación."""
    with ui.row().classes("items-center gap-0 no-wrap"):
        ui.label(texto_etiqueta).classes(clases)
        ayuda(texto_ayuda)
