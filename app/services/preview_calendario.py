"""Vista previa: qué vencimientos genera una regla, ANTES de guardarla.

# Por qué existe

Definir la obligación es el paso más importante del sistema: de ahí salen el
calendario, las tareas, las alertas y la trazabilidad. Y hasta ahora se definía
**a ciegas** — se elegía periodicidad, día de corte y ajuste a día hábil, se
guardaba, y las fechas resultantes solo se veían cuando el planificador ya había
creado las tareas.

Una regla mal entendida no falla ruidosamente: genera vencimientos distintos de
los que la persona creía, o ninguno. Con este módulo, quien crea la obligación
ve la lista de fechas antes de confirmar.

# Usa el MOTOR REAL, y eso no es negociable

Llama a `GeneradorTareas.obtener_deadlines_para_fecha`, la misma función que usa
el planificador en producción. Una vista previa con su propia copia de la lógica
sería peor que no tenerla: mostraría fechas que el sistema luego no genera, y
nadie sabría cuál de las dos está mal.

Este módulo NO calcula fechas. Solo barre el calendario y recoge lo que el motor
devuelve.
"""
from __future__ import annotations

from dataclasses import dataclass
from datetime import date, datetime, timedelta

from app.services.scheduler import GeneradorTareas

#: Tope de días a barrer. Un año y medio cubre cualquier periodicidad razonable
#: —incluida la anual, que necesita cruzar el año— sin volver lento el formulario.
DIAS_MAXIMOS = 550

#: Nombres de los días, para explicar el ajuste a hábil en la interfaz.
DIAS_ES = ("lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo")
MESES_ES = ("enero", "febrero", "marzo", "abril", "mayo", "junio", "julio",
            "agosto", "septiembre", "octubre", "noviembre", "diciembre")


@dataclass(frozen=True)
class Vencimiento:
    fecha: date
    #: Fecha que habría tocado sin el ajuste a día hábil, si difiere.
    sin_ajustar: date | None = None

    @property
    def se_ajusto(self) -> bool:
        return self.sin_ajustar is not None and self.sin_ajustar != self.fecha

    def describir(self) -> str:
        d = self.fecha
        txt = f"{DIAS_ES[d.weekday()]} {d.day} de {MESES_ES[d.month-1]} de {d.year}"
        if self.se_ajusto:
            o = self.sin_ajustar
            txt += (f"  (movido desde {DIAS_ES[o.weekday()]} "
                    f"{o.day} de {MESES_ES[o.month-1]})")
        return txt


def _sin_ajuste(config: dict) -> dict:
    """La misma configuración, pero sin mover a día hábil.

    Sirve para poder DECIRLE al usuario que una fecha se movió y desde dónde.
    Un vencimiento que aparece un viernes cuando la regla decía «día 1» no se
    entiende solo; con el origen al lado, sí.
    """
    limpia = dict(config)
    for bandera in ("mover_habil", "mover_habil_anterior",
                    "ajuste_habil", "ajuste_habil_anterior"):
        limpia.pop(bandera, None)
    return limpia


def proximos(config: dict, cuantos: int = 6, desde: date | None = None
             ) -> tuple[list[Vencimiento], str]:
    """Los próximos `cuantos` vencimientos que genera `config`.

    Devuelve (lista, advertencia). La advertencia es texto vacío si todo va
    bien; si no, explica por qué la lista salió corta o vacía — que es la
    información que de verdad hace falta cuando una regla no produce nada.
    """
    desde = desde or date.today()

    # La regla puede empezar en el futuro: barrer desde hoy no encontraría nada
    # y parecería que no genera vencimientos.
    inicio_cfg = config.get("fecha_inicio")
    if inicio_cfg:
        try:
            f = datetime.strptime(str(inicio_cfg), "%Y-%m-%d").date()
            if f > desde:
                desde = f
        except (ValueError, TypeError):
            return ([], "La fecha de inicio no se entiende: use el formato AAAA-MM-DD.")

    if config.get("modo") == "HOURLY":
        return ([], "La periodicidad por horas genera varios vencimientos cada "
                    "día; la vista previa por fechas no aplica.")

    limpia = _sin_ajuste(config)
    encontrados: list[Vencimiento] = []
    dia = desde
    for _ in range(DIAS_MAXIMOS):
        if len(encontrados) >= cuantos:
            break
        try:
            hay = GeneradorTareas.obtener_deadlines_para_fecha(dia, config)
        except Exception as e:  # noqa: BLE001
            # Se informa en vez de devolver una lista vacía: «no genera nada» y
            # «la regla revienta» son cosas distintas y confundirlas dejaría al
            # usuario creyendo que configuró algo inofensivo.
            return (encontrados, f"La regla no se pudo evaluar el {dia}: {e}")
        if hay:
            sin = None
            if limpia != config:
                # ¿Qué día habría tocado sin ajuste? Se busca en la ventana
                # cercana, que es donde puede estar tras mover a hábil.
                for delta in range(-6, 7):
                    cand = dia + timedelta(days=delta)
                    if GeneradorTareas.obtener_deadlines_para_fecha(cand, limpia):
                        sin = cand
                        break
            encontrados.append(Vencimiento(fecha=dia, sin_ajustar=sin))
        dia += timedelta(days=1)

    if not encontrados:
        return ([], "Esta regla no genera ningún vencimiento en los próximos "
                    f"{DIAS_MAXIMOS} días. Revise la periodicidad, los meses "
                    "seleccionados y la fecha de inicio.")
    if len(encontrados) < cuantos:
        return (encontrados, f"Solo se encontraron {len(encontrados)} "
                             f"vencimiento(s) en los próximos {DIAS_MAXIMOS} días.")
    return (encontrados, "")
