"""El asistente de cronograma: el calendario pregunta, la lista responde.

# La decisión de diseño

Hay dos maneras de preguntarle a alguien cada cuánto vence un informe:

  **una lista** — «elija: diaria, semanal, mensual, trimestral…»
  **un calendario** — «márqueme cuándo vence»

La lista pide traducir. Quien crea la obligación tiene delante una resolución
que dice «dentro de los primeros cinco días hábiles del mes siguiente», y para
contestar la lista primero tiene que saber que eso es MENSUAL y después que el
día se expresa contando hábiles y no corridos. Es pedirle que haga el trabajo
del sistema, y el error que produce no se ve: eligió «mensual, día 5» y el 5 no
es el 5º hábil.

El calendario no pide traducir nada. Marca los días que tiene delante.

# Pero el calendario solo no basta, y ahí está lo importante

Marcar el lunes 6 de julio de 2026 **no determina la obligación**. Puede ser
diaria desde ese día, todos los lunes, cada dos lunes, el 6 de cada mes, el 1er
lunes de cada mes, el 4º día hábil de cada mes, cada 6 de julio, o una sola vez.
Son ocho obligaciones distintas y la selección es idéntica en las ocho.

Por eso la lista no desaparece: **cambia de sitio**. Deja de ser la pregunta y
pasa a ser la respuesta. Y las opciones ya no son un catálogo abstracto sino las
lecturas de lo que esa persona acaba de marcar, cada una con las fechas que
produciría. El sistema no adivina cuál es —adivinarla sería inventar la
obligación— pero acota de trece a las que de verdad caben.

# Por qué esto es un servicio y no una pantalla

Cada página del asistente **valida lo que se lleva contestado**, y esa revisión
tiene que ser la misma que la del formulario clásico: dos juegos de reglas
garantizan que uno esté mal. Aquí no hay reglas propias — se llama a
`validar_regla.validar` con los campos que ya se han tenido ocasión de
contestar (`campos_hasta`), que es justo para lo que existe.

Y separarlo de NiceGUI permite comprobarlo: `tests/test_asistente_regla.py`
recorre el asistente entero sin abrir un navegador.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from datetime import date, timedelta

from app.services.inferir_periodicidad import Lectura, interpretaciones
from app.services.validar_regla import Resultado, campos_hasta, validar

#: Las cuatro páginas. El orden no es cosmético: cada una decide lo que la
#: siguiente necesita, y ninguna pregunta algo que la anterior ya haya fijado.
MARCAR, LECTURA, DETALLE, CONFIRMAR = 0, 1, 2, 3

PAGINAS = (
    ("Cuándo vence",
     "Marque en el calendario los días en que este informe vence. No hace falta "
     "marcarlos todos: con dos o tres el sistema reconoce el patrón."),
    ("Qué significa",
     "Lo que marcó puede significar varias cosas distintas. Elija cuál es: el "
     "sistema no lo supone, porque suponerlo sería inventar la obligación."),
    ("Cómo se ajusta",
     "El día de vencimiento no cambia nunca — es el que fija la norma. Lo que "
     "se decide aquí es qué pasa cuando ese día no es hábil."),
    ("Confirmar",
     "Desde cuándo rige y qué va a generar. Estas son las fechas de verdad, "
     "calculadas con el mismo motor que crea las tareas."),
)

#: Qué hacer cuando el día nominal no es hábil. Son CUATRO cosas distintas, no
#: cuatro maneras de mover una fecha.
AJUSTES = {
    "ninguno": ("Se entrega ese mismo día, sea hábil o no",
                "El vencimiento no se toca y no hay plazo extra."),
    "anterior": ("Se entrega el día hábil anterior",
                 "El vencimiento sigue siendo el mismo: lo que se adelanta es la "
                 "entrega, porque ese día no trabaja nadie. NO concede plazo. Es "
                 "la opción segura para un plazo legal: adelantar nunca llega "
                 "tarde, aplazar sí."),
    "siguiente": ("Hay plazo de gracia hasta el siguiente día hábil",
                  "Esta sí amplía el plazo: entregar ese día NO cuenta como "
                  "incumplimiento. Úsela solo si la norma o el contrato lo "
                  "conceden."),
    "omitir": ("Ese día no hay informe",
               "El vencimiento no existe: no se mueve ni antes ni después. Es lo "
               "que significa «diario en días laborales» — un festivo no genera "
               "un informe que haya que entregar otro día, genera ninguno."),
}

#: Qué pareja de banderas escribe cada modo.
#:
#: El motor lee las dos —`ajuste_habil*` y `mover_habil*`— pero el formulario
#: clásico solo entiende la suya, y una regla creada aquí tiene que poder
#: abrirse allí sin que el ajuste desaparezca por el camino. Escribir las dos
#: parejas sería peor: dos maneras de decir lo mismo, y dentro de un año nadie
#: sabría cuál manda.
_PAREJAS = {
    "ESPECIFICO": ("ajuste_habil", "ajuste_habil_anterior"),
    "FECHAS": ("ajuste_habil", "ajuste_habil_anterior"),
    "CICLICO": ("mover_habil", "mover_habil_anterior"),
    "SEMANAL": ("mover_habil", "mover_habil_anterior"),
}


def banderas_de_ajuste(modo: str, ajuste: str) -> dict:
    """La opción elegida, en las banderas que el motor lee para ese modo.

    Las tres claves se emiten SIEMPRE, con su valor explícito. Omitir la que va
    en falso dejaría el valor anterior si alguien fusiona diccionarios, y ese es
    el tipo de resto que hace que una obligación cambie de comportamiento sin
    que nadie la haya tocado.
    """
    gracia, anticipado = _PAREJAS.get(modo, ("ajuste_habil", "ajuste_habil_anterior"))
    return {gracia: ajuste == "siguiente",
            anticipado: ajuste == "anterior",
            "omitir_festivos": ajuste == "omitir"}


def dias_entre(desde: date, hasta: date) -> list[date]:
    """Todos los días del rango, extremos incluidos. Vacío si va al revés.

    Marcar día a día una cadena de dos semanas son catorce clics. Esto existe
    para que el calendario admita también «del 1 al 15», que es como se dice.
    """
    if hasta < desde:
        return []
    return [desde + timedelta(days=i) for i in range((hasta - desde).days + 1)]


@dataclass
class Asistente:
    """El estado de la creación de un cronograma, página a página.

    No toca la base de datos ni la interfaz: produce una regla y dice, en cada
    paso, si lo que se lleva contestado se sostiene.
    """

    #: Los días marcados en el calendario. La PREGUNTA.
    seleccion: list[date] = field(default_factory=list)
    #: Índice de la lectura elegida dentro de `lecturas()`. La RESPUESTA.
    eleccion: int | None = None
    ajuste: str = "ninguno"
    #: Solo cuando la lectura es «varias veces al día».
    franja: dict = field(default_factory=lambda: {
        "hora_inicio": 8, "hora_fin": 18, "intervalo_horas": 1})
    #: Desde cuándo rige. `None` = la primera fecha marcada.
    vigencia: date | None = None
    paso: int = MARCAR

    _cache: tuple = field(default=(), repr=False)

    # ---------------------------------------------------------------- página 1
    def marcar(self, dia: date) -> None:
        """Añade o quita un día. El calendario es un interruptor por día."""
        if dia in self.seleccion:
            self.seleccion.remove(dia)
        else:
            self.seleccion.append(dia)
        self._invalidar()

    def marcar_rango(self, desde: date, hasta: date) -> int:
        """Marca la cadena completa. Devuelve cuántos días añadió."""
        nuevos = [d for d in dias_entre(desde, hasta) if d not in self.seleccion]
        self.seleccion += nuevos
        self._invalidar()
        return len(nuevos)

    def limpiar_seleccion(self) -> None:
        self.seleccion = []
        self._invalidar()

    def fijar_seleccion(self, fechas) -> bool:
        """Reemplaza lo marcado. Devuelve si de verdad cambió algo.

        Un calendario avisa de su valor también cuando se repinta, y sin esta
        comprobación volver atrás y adelante bastaría para borrar la lectura
        elegida sin que nadie hubiera tocado nada. Comparar antes de invalidar
        es lo que separa «el usuario cambió las fechas» de «la pantalla se
        volvió a dibujar».
        """
        nuevas = sorted(set(fechas))
        if nuevas == self.seleccion:
            return False
        self.seleccion = nuevas
        self._invalidar()
        return True

    def _invalidar(self) -> None:
        """Cambió lo marcado: las lecturas anteriores ya no son de esto.

        Se borra también la elección. Conservarla sería el defecto silencioso de
        siempre: el usuario cambia una fecha, la lectura elegida deja de
        corresponder a lo que hay en pantalla, y la obligación se guarda diciendo
        algo que él no eligió.
        """
        self.seleccion = sorted(set(self.seleccion))
        self.eleccion = None
        self._cache = ()

    # ---------------------------------------------------------------- página 2
    def lecturas(self) -> list[Lectura]:
        """Qué puede ser lo que se marcó. Cada una se comprobó EJECUTÁNDOLA."""
        if not self.seleccion:
            return []
        clave = tuple(self.seleccion)
        if self._cache and self._cache[0] == clave:
            return self._cache[1]
        lista = interpretaciones(self.seleccion)
        # «Varias veces al día» no sale de marcar días —el calendario no tiene
        # horas— pero es una lectura legítima de lo mismo, y sin ella un
        # administrador con un informe por turnos no puede usar el asistente.
        # Se ofrece exactamente cuando «todos los días» se ofrece: si lo marcado
        # admite leerse como diario, admite leerse como varias veces al día.
        if any(L.periodicidad == "DIARIA" for L in lista):
            lista = lista + [Lectura(
                periodicidad="VARIAS_AL_DIA",
                nombre="Varias veces al día",
                dice="varias veces cada día, en la franja horaria que indique",
                regla={"modo": "HOURLY",
                       "fecha_inicio": min(self.seleccion).isoformat()},
                proximas=[])]
        self._cache = (clave, lista)
        return lista

    @property
    def lectura(self) -> Lectura | None:
        lista = self.lecturas()
        if self.eleccion is None or not 0 <= self.eleccion < len(lista):
            return None
        return lista[self.eleccion]

    def elegir(self, indice: int) -> None:
        self.eleccion = indice

    @property
    def es_por_horas(self) -> bool:
        lect = self.lectura
        return lect is not None and lect.regla.get("modo") == "HOURLY"

    # ---------------------------------------------------------------- la regla
    def regla(self) -> dict:
        """La regla completa, tal como se guardaría. `{}` si falta elegir."""
        lect = self.lectura
        if lect is None:
            return {}
        base = dict(lect.regla)
        if self.es_por_horas:
            # El ajuste a día hábil no aplica: una franja horaria no se traslada
            # a otro día. Ofrecerlo aquí sería dar una opción que no hace nada.
            base.update({k: int(v) for k, v in self.franja.items()})
        else:
            base.update(banderas_de_ajuste(base.get("modo", ""), self.ajuste))
        if self.vigencia:
            base["fecha_inicio"] = self.vigencia.isoformat()
        return base

    def vigencia_efectiva(self) -> date | None:
        if self.vigencia:
            return self.vigencia
        return min(self.seleccion) if self.seleccion else None

    # ------------------------------------------------------------- validación
    def revisar(self) -> Resultado:
        """La revisión del paso actual, con las MISMAS reglas del formulario.

        Hasta la última página se limita a los campos que el usuario ya ha
        tenido ocasión de contestar: quejarse en la página 2 de un ajuste que se
        pregunta en la 3 no es ser estricto, es acusar a alguien de no haber
        contestado algo que nadie le ha preguntado. Y entrena a saltarse los
        avisos antes incluso de terminar.
        """
        regla = self.regla()
        if not regla:
            return Resultado()
        if self.paso >= CONFIRMAR:
            # La revisión completa, incluida la de que el nombre no miente y la
            # de que la regla genera algo.
            return validar(regla, periodicidad=self.lectura.periodicidad)
        return validar(regla, respondidos=campos_hasta(self.paso))

    def problemas(self) -> list[str]:
        """Lo que impide pasar de esta página. Vacío = se puede seguir."""
        if self.paso == MARCAR:
            if not self.seleccion:
                return ["Marque al menos un día en el calendario: es la pregunta "
                        "de la que sale todo lo demás."]
            return []
        if self.lectura is None:
            return ["Elija qué significa lo que marcó. El sistema no lo supone: "
                    "las mismas fechas pueden ser varias obligaciones distintas."]
        return self.revisar().errores

    def puede_avanzar(self) -> bool:
        return not self.problemas()

    def avanzar(self) -> bool:
        if self.paso >= CONFIRMAR or not self.puede_avanzar():
            return False
        self.paso += 1
        return True

    def retroceder(self) -> bool:
        if self.paso <= MARCAR:
            return False
        self.paso -= 1
        return True

    # ------------------------------------------------------------- la promesa
    def proyeccion(self, cuantos: int = 12, desde: date | None = None):
        """Los próximos vencimientos, con el motor de producción.

        Es lo único que convierte la elección en una decisión informada: el
        nombre de una periodicidad se puede malinterpretar, una lista de fechas
        no.
        """
        from app.services.preview_calendario import proximos

        return proximos(self.regla(), cuantos=cuantos,
                        desde=desde or self.vigencia_efectiva() or date.today())

    def cuantos_caen_en_no_habil(self, cuantos: int = 24) -> tuple[int, int]:
        """(cuántos de los próximos vencimientos NO caen en día hábil, total).

        Se MIDE sobre el calendario real en vez de explicar el ajuste en
        abstracto. «Elija qué pasa si cae en día no hábil» es una pregunta
        hipotética; «3 de los próximos 24 caen en día no hábil» es la respuesta
        a por qué le estamos preguntando. Y cuando salen cero, decirlo también
        importa: la elección sigue haciendo falta porque el calendario cambia
        cada año, y ese es justo el motivo por el que el sistema existe.
        """
        from app.services.calendar_engine import MotorReglasDinamicas

        if self.es_por_horas:
            return (0, 0)
        # Se mide sobre la regla SIN ajuste: la pregunta es cuántos días
        # NOMINALES caen en no hábil, y con la omisión puesta esos días
        # desaparecerían del recuento justo por lo que se quiere contar.
        limpia = {k: v for k, v in self.regla().items()
                  if k not in ("ajuste_habil", "ajuste_habil_anterior",
                               "mover_habil", "mover_habil_anterior",
                               "omitir_festivos")}
        from app.services.preview_calendario import proximos

        venc, _ = proximos(limpia, cuantos=cuantos,
                           desde=self.vigencia_efectiva() or date.today())
        motor = MotorReglasDinamicas()
        return (sum(1 for v in venc if not motor._es_habil(v.nominal)), len(venc))

    def resumen(self) -> str:
        """Una línea con lo que se acaba de definir, para confirmarlo."""
        lect = self.lectura
        if lect is None:
            return ""
        texto = f"{lect.nombre}: {lect.dice}"
        if self.es_por_horas:
            f = self.franja
            return (f"{texto} — de las {int(f['hora_inicio'])} a las "
                    f"{int(f['hora_fin'])}, cada {int(f['intervalo_horas'])} h")
        if self.ajuste != "ninguno":
            return f"{texto} · {AJUSTES[self.ajuste][0].lower()}"
        return texto
