"""El asistente de cronograma, en pantalla. Cuatro páginas.

La lógica no está aquí: está en `app/services/asistente_regla.py`, y se puede
recorrer entera sin abrir un navegador. Este módulo solo pinta lo que aquel
decide, y esa separación es lo que permite que la línea lógica esté probada.

# Las cuatro páginas

    1. Cuándo vence ....... un CALENDARIO. Se marcan los días.
    2. Qué significa ...... una LISTA. Las lecturas de lo marcado.
    3. Cómo se ajusta ..... qué pasa cuando el día no es hábil.
    4. Confirmar .......... desde cuándo rige y las fechas que va a generar.

El orden importa y es al revés de lo habitual: primero las fechas, después el
nombre. Quien crea la obligación tiene delante una resolución con fechas, no un
catálogo de periodicidades.

# Ninguna página deja pasar lo que la siguiente no pueda arreglar

El botón de avanzar se apaga y **se dice por qué**. Un botón gris sin
explicación es una puerta cerrada sin cartel: quien la encuentra prueba cosas al
azar hasta que se abre, y acaba guardando cualquier cosa que la abra.
"""
from datetime import date, datetime

from nicegui import ui

from app.services.asistente_regla import (AJUSTES, CONFIRMAR, DETALLE, LECTURA,
                                          MARCAR, PAGINAS, Asistente)
from app.services.preview_calendario import dia_corto


def _significado(lectura) -> str:
    """Qué produce esta periodicidad, dicho para una persona.

    Se enseña SIEMPRE, no solo cuando dos nombres colisionan. Que el significado
    aparezca únicamente al desambiguar deja a alguien eligiendo «Trimestral» sin
    ver que son cuatro entregas al año, y el error se descubre un trimestre
    después.
    """
    from app.services.periodicidad import CATALOGO

    if lectura.periodicidad in CATALOGO:
        return CATALOGO[lectura.periodicidad].significado()
    # Sin nombre de catálogo no hay significado enlatado: lo dice la propia
    # lectura, que para eso está escrita en castellano.
    return "una periodicidad propia, sin nombre estándar"


def _fecha(texto):
    try:
        return datetime.strptime(str(texto), "%Y-%m-%d").date()
    except (ValueError, TypeError):
        return None


def abrir_asistente(al_confirmar, regla_actual=None) -> None:
    """Abre el asistente. Al terminar llama a `al_confirmar(regla, resumen)`.

    `regla_actual` no se intenta traducir de vuelta a una selección de días: eso
    exigiría adivinar qué fechas marcó quien la creó, y adivinarlas produciría
    una selección que no es la suya. Si hay una regla puesta, se avisa de que
    terminar el asistente la reemplaza — que es lo que va a pasar de verdad.
    """
    a = Asistente()

    with ui.dialog().props("persistent") as dialogo, \
            ui.card().classes("w-[860px] max-w-full p-0 gap-0"):

        # ---- cabecera: en qué página estamos ------------------------------
        with ui.row().classes("w-full bg-slate-900 text-white px-4 py-3 "
                              "items-center justify-between no-wrap"):
            ui.label("Definir el cronograma").classes("text-lg font-bold")
            ui.button(icon="close", on_click=dialogo.close) \
                .props("flat round dense text-color=white")
        pasos_fila = ui.row().classes(
            "w-full bg-slate-100 px-4 py-2 gap-1 items-center no-wrap")

        cuerpo = ui.column().classes(
            "w-full p-5 gap-3 scroll-y max-h-[62vh] min-h-[380px]")
        pie = ui.row().classes(
            "w-full px-4 py-3 bg-gray-50 border-t items-center justify-between "
            "no-wrap gap-3")

    # ----------------------------------------------------------------- pintar
    def pintar():
        _pintar_pasos()
        cuerpo.clear()
        with cuerpo:
            titulo, explicacion = PAGINAS[a.paso]
            ui.label(titulo).classes("text-xl font-bold text-slate-800")
            ui.label(explicacion).classes("text-sm text-slate-500 -mt-1")
            ui.separator().classes("my-1")
            if a.paso == MARCAR:
                _pagina_marcar()
            elif a.paso == LECTURA:
                _pagina_lectura()
            elif a.paso == DETALLE:
                _pagina_detalle()
            else:
                _pagina_confirmar()
        _pintar_pie()

    def _pintar_pasos():
        pasos_fila.clear()
        with pasos_fila:
            for i, (titulo, _) in enumerate(PAGINAS):
                actual = i == a.paso
                hecho = i < a.paso
                with ui.row().classes("items-center gap-1 no-wrap"):
                    ui.icon("check_circle" if hecho else
                            "radio_button_checked" if actual else
                            "radio_button_unchecked").classes(
                        "text-sm " + ("text-emerald-600" if hecho else
                                      "text-slate-800" if actual else
                                      "text-slate-300"))
                    ui.label(f"{i + 1}. {titulo}").classes(
                        "text-xs " + ("font-bold text-slate-800" if actual else
                                      "text-slate-500"))
                if i < len(PAGINAS) - 1:
                    ui.icon("chevron_right").classes("text-slate-300 text-sm")

    # -------------------------------------------------------------- página 1
    def _pagina_marcar():
        def desde_calendario(e):
            valores = e.value or []
            if isinstance(valores, str):
                valores = [valores]
            fechas = [f for f in (_fecha(v) for v in valores if isinstance(v, str))
                      if f]
            # Solo se repinta si de verdad cambió algo: un calendario avisa de su
            # valor también al dibujarse, y repintar en ese caso sería un bucle.
            if a.fijar_seleccion(fechas):
                pintar()

        def marcar_rango():
            d, h = _fecha(rango["desde"]), _fecha(rango["hasta"])
            if not d or not h:
                ui.notify("Indique las dos fechas del rango", type="warning")
                return
            if h < d:
                # Se dice qué pasa, no «fechas inválidas».
                ui.notify("La fecha final es anterior a la inicial: no hay "
                          "ningún día entre las dos.", type="warning")
                return
            ui.notify(f"Marcados {a.marcar_rango(d, h)} día(s)", type="positive")
            pintar()

        rango = {"desde": "", "hasta": ""}
        with ui.row().classes("w-full gap-5 items-start no-wrap"):
            ui.date(value=[d.isoformat() for d in a.seleccion],
                    on_change=desde_calendario) \
                .props("multiple minimal today-btn") \
                .classes("shadow-none border rounded")

            with ui.column().classes("flex-1 gap-2"):
                # Marcar día a día una cadena de dos semanas son catorce clics.
                ui.label("…o marque una cadena de días de una vez") \
                    .classes("text-xs font-bold text-slate-600")
                with ui.row().classes("w-full gap-2 items-center no-wrap"):
                    ui.input("Del").props("type=date dense outlined") \
                        .bind_value(rango, "desde").mark("rango-desde") \
                        .classes("flex-1")
                    ui.input("al").props("type=date dense outlined") \
                        .bind_value(rango, "hasta").mark("rango-hasta") \
                        .classes("flex-1")
                ui.button("Marcar el rango", icon="date_range",
                          on_click=marcar_rango) \
                    .props("outline dense").classes("w-full")

                ui.separator().classes("my-1")
                _marcadas()

    def _marcadas():
        if not a.seleccion:
            ui.label("Nada marcado todavía.").classes("text-xs text-slate-400")
            return
        with ui.row().classes("w-full items-center justify-between no-wrap"):
            ui.label(f"{len(a.seleccion)} día(s) marcado(s)") \
                .classes("text-xs font-bold text-slate-600")
            ui.button("Quitar todos", icon="clear",
                      on_click=lambda: (a.limpiar_seleccion(), pintar())) \
                .props("flat dense size=sm color=grey")
        with ui.row().classes("w-full flex-wrap gap-1"):
            for d in a.seleccion:
                ui.button(dia_corto(d), icon="close",
                          on_click=lambda _, dia=d: (a.marcar(dia), pintar())) \
                    .props("outline dense size=sm no-caps color=primary")
        ui.label("Con dos o tres días de meses distintos el sistema distingue "
                 "patrones que con uno solo no puede.") \
            .classes("text-xs text-slate-400 mt-1")

    # -------------------------------------------------------------- página 2
    def _pagina_lectura():
        lecturas = a.lecturas()
        ui.label(f"Marcó {len(a.seleccion)} día(s). Eso puede significar "
                 f"{len(lecturas)} obligaciones distintas:") \
            .classes("text-sm text-slate-600")

        for i, L in enumerate(lecturas):
            elegida = a.eleccion == i

            def elegir(_=None, indice=i):
                a.elegir(indice)
                pintar()

            # El marcador no se ve: es lo que permite que una prueba pulse ESTA
            # lectura y no otra. Sin él solo se puede pulsar la etiqueta, que no
            # lleva el manejador, y el recorrido completo no se puede comprobar.
            with ui.card().on("click", elegir).mark(f"lectura:{L.dice}").classes(
                    "w-full p-3 cursor-pointer transition-all " +
                    ("border-2 border-emerald-500 bg-emerald-50"
                     if elegida else "border border-slate-200 hover:bg-slate-50")):
                with ui.row().classes("w-full items-start gap-3 no-wrap"):
                    ui.icon("check_circle" if elegida else "radio_button_unchecked") \
                        .classes("text-xl mt-1 " + ("text-emerald-600" if elegida
                                                    else "text-slate-300"))
                    with ui.column().classes("gap-0 flex-1"):
                        ui.label(L.dice).classes("font-bold text-slate-800")
                        ui.label(f"{L.nombre} — {_significado(L)}") \
                            .classes("text-xs text-slate-500")
                        if L.proximas:
                            ui.label("Produciría: " + ", ".join(
                                dia_corto(f) for f in L.proximas) + "…") \
                                .classes("text-xs text-slate-400 mt-1")

        ui.label("El sistema no elige por usted: las mismas fechas caben en "
                 "varias obligaciones y suponer cuál sería inventarla.") \
            .classes("text-xs text-slate-400 mt-2")

    # -------------------------------------------------------------- página 3
    def _pagina_detalle():
        if a.es_por_horas:
            _franja_horaria()
            return

        caen, total = a.cuantos_caen_en_no_habil(cuantos=24)
        with ui.row().classes("w-full items-center gap-2 bg-slate-50 p-3 rounded"):
            ui.icon("insights").classes("text-slate-500")
            if caen:
                ui.label(f"De los próximos {total} vencimientos, {caen} caen en "
                         f"día no hábil. Esta elección decide qué pasa con esos.") \
                    .classes("text-sm text-slate-700")
            else:
                # Que hoy salgan cero no vuelve la pregunta inútil: el calendario
                # de festivos cambia cada año, y que la regla siga siendo
                # correcta dentro de diez es justo para lo que sirve esto.
                ui.label(f"Ninguno de los próximos {total} vencimientos cae en "
                         f"día no hábil. Aun así hay que decidirlo: el calendario "
                         f"cambia cada año y la regla se escribe una sola vez.") \
                    .classes("text-sm text-slate-700")

        for clave, (titulo, explicacion) in AJUSTES.items():
            elegida = a.ajuste == clave

            def elegir(_=None, valor=clave):
                a.ajuste = valor
                pintar()

            with ui.card().on("click", elegir).mark(f"ajuste:{clave}").classes(
                    "w-full p-3 cursor-pointer " +
                    ("border-2 border-emerald-500 bg-emerald-50"
                     if elegida else "border border-slate-200 hover:bg-slate-50")):
                with ui.row().classes("w-full items-start gap-3 no-wrap"):
                    ui.icon("check_circle" if elegida else "radio_button_unchecked") \
                        .classes("text-xl mt-1 " + ("text-emerald-600" if elegida
                                                    else "text-slate-300"))
                    with ui.column().classes("gap-0 flex-1"):
                        ui.label(titulo).classes("font-bold text-slate-800")
                        ui.label(explicacion).classes("text-xs text-slate-500")

    def _franja_horaria():
        ui.label("Una franja horaria no se traslada a otro día, así que aquí no "
                 "hay ajuste a día hábil que ofrecer: se pregunta la franja.") \
            .classes("text-sm text-slate-600")
        with ui.row().classes("w-full gap-3 no-wrap"):
            for campo, etiqueta, tope in (("hora_inicio", "Hora de inicio", 23),
                                          ("hora_fin", "Hora final", 23),
                                          ("intervalo_horas", "Cada cuántas horas", 12)):
                ui.number(etiqueta, min=0 if tope == 23 else 1, max=tope,
                          format="%.0f", on_change=lambda _: _pintar_pie()) \
                    .bind_value(a.franja, campo).props("dense outlined").classes("flex-1")
        ui.label("Si la hora final es menor que la de inicio, es un turno que "
                 "cruza la medianoche. Se admite: el sistema tiene que aceptar la "
                 "forma que tenga la obligación, no al revés.") \
            .classes("text-xs text-slate-400")

    # -------------------------------------------------------------- página 4
    def _pagina_confirmar():
        with ui.row().classes("w-full items-center gap-3 no-wrap"):
            ui.label("Rige desde:").classes("text-sm font-bold text-slate-700")
            ui.input(value=(a.vigencia_efectiva() or date.today()).isoformat(),
                     on_change=lambda e: (
                         setattr(a, "vigencia", _fecha(e.value)), pintar())) \
                .props("type=date dense outlined").mark("vigencia").classes("w-48")
            ui.label("por defecto, el primer día que marcó") \
                .classes("text-xs text-slate-400")

        with ui.card().classes("w-full bg-slate-900 text-white p-3"):
            ui.label(a.resumen()).classes("text-base font-bold")

        revision = a.revisar()
        for error in revision.errores:
            with ui.row().classes("w-full items-start gap-2 bg-red-50 p-3 rounded "
                                  "border border-red-200 no-wrap"):
                ui.icon("error").classes("text-red-600 mt-0.5")
                ui.label(error).classes("text-sm text-red-800")
        for aviso in revision.advertencias:
            with ui.row().classes("w-full items-start gap-2 bg-amber-50 p-3 rounded "
                                  "border border-amber-200 no-wrap"):
                ui.icon("warning").classes("text-amber-600 mt-0.5")
                ui.label(aviso).classes("text-sm text-amber-800")

        if a.es_por_horas:
            ui.label("La periodicidad por horas genera varios vencimientos cada "
                     "día; la proyección por fechas no aplica.") \
                .classes("text-sm text-slate-500")
            return

        ui.label("Lo que va a generar").classes("text-sm font-bold text-slate-700 mt-2")
        venc, aviso = a.proyeccion(cuantos=12)
        if aviso:
            ui.label(aviso).classes("text-xs text-amber-700")
        with ui.column().classes("w-full gap-0 border rounded divide-y"):
            for v in venc:
                with ui.row().classes("w-full items-center gap-2 px-3 py-1.5 no-wrap"):
                    ui.icon("event").classes(
                        "text-sm " + ("text-amber-600" if v.se_ajusto
                                      else "text-slate-400"))
                    ui.label(v.describir()).classes(
                        "text-xs " + ("text-amber-800" if v.se_ajusto
                                      else "text-slate-600"))
        ui.label("Calculadas con el mismo motor que crea las tareas. No es una "
                 "estimación: son las fechas.").classes("text-xs text-slate-400")

    # ------------------------------------------------------------------- pie
    def _pintar_pie():
        pie.clear()
        with pie:
            problemas = a.problemas()
            with ui.column().classes("gap-0 flex-1"):
                if problemas:
                    # El botón se apaga y se dice POR QUÉ. Un botón gris sin
                    # explicación se acaba abriendo probando cosas al azar.
                    for p in problemas[:2]:
                        ui.label(p).classes("text-xs text-red-700")
                elif a.paso == CONFIRMAR:
                    for aviso in a.revisar().advertencias[:1]:
                        ui.label(aviso).classes("text-xs text-amber-700")
            with ui.row().classes("gap-2 no-wrap"):
                if a.paso > MARCAR:
                    ui.button("Atrás", icon="arrow_back",
                              on_click=lambda: (a.retroceder(), pintar())) \
                        .props("flat")
                if a.paso < CONFIRMAR:
                    ui.button("Siguiente",
                              on_click=lambda: (a.avanzar(), pintar())) \
                        .props("icon-right=arrow_forward") \
                        .classes("bg-slate-900 text-white") \
                        .set_enabled(not problemas)
                else:
                    ui.button("Usar este cronograma", icon="check",
                              on_click=confirmar) \
                        .mark("confirmar-cronograma") \
                        .classes("bg-emerald-600 text-white") \
                        .set_enabled(not problemas)

    def confirmar():
        # Esta guarda y el botón apagado no son dos mecanismos: son el mismo
        # `problemas()` usado para dos cosas distintas — decírselo a quien mira
        # la pantalla, y negarse a entregar la regla. Por la interfaz no se
        # alcanza (el botón está deshabilitado, y una mutación lo comprueba);
        # está aquí porque lo que se entrega es una obligación, y el sitio donde
        # se decide no puede depender de que la pintura sea correcta.
        if a.problemas():
            return
        # Se CIERRA antes de avisar, y el orden importa. Quien recibe la regla
        # normalmente redibuja la pantalla que contiene este diálogo, así que al
        # volver aquí el diálogo ya no existe y cerrarlo entonces es tocar un
        # elemento borrado.
        dialogo.close()
        al_confirmar(a.regla(), a.resumen())

    pintar()
    if regla_actual:
        ui.notify("Este informe ya tiene un cronograma. Si termina el asistente, "
                  "lo reemplaza.", type="info", timeout=6000)
    dialogo.open()
