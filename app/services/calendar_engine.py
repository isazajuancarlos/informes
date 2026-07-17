from datetime import date, datetime, timedelta
import holidays
from dateutil.rrule import rrule, DAILY, WEEKLY, MONTHLY, YEARLY, MO, TU, WE, TH, FR, SA, SU

# Mapeo para convertir strings JSON a constantes de dateutil
FRECUENCIAS = {
    "DAILY": DAILY, "WEEKLY": WEEKLY, "MONTHLY": MONTHLY, "YEARLY": YEARLY
}
DIAS_SEMANA = {
    "MO": MO, "TU": TU, "WE": WE, "TH": TH, "FR": FR, "SA": SA, "SU": SU
}

class MotorReglasDinamicas:
    def __init__(self, pais_iso: str = None):
        from app.core.config import PAIS_ISO as CONFIG_PAIS_ISO
        effective_pais = pais_iso or CONFIG_PAIS_ISO
        try:
            self.festivos = holidays.country_holidays(effective_pais)
        except Exception:
            print(f"[CalendarEngine] País '{effective_pais}' no soportado. Usando calendario sin festivos.")
            self.festivos = {}

    def _es_habil(self, fecha: date) -> bool:
        """Verifica si es hábil (Lunes-Viernes y No Festivo)"""
        return (fecha.weekday() < 5) and (fecha not in self.festivos)

    def _ajustar_fecha(self, fecha: date, logica_ajuste: str) -> date:
        """
        Ajusta una fecha individual según la regla dinámica de conflicto.
        logica_ajuste: 'NEXT_WORKDAY' (>), 'PREV_WORKDAY' (<), 'KEEP' (=)
        """
        if self._es_habil(fecha) or logica_ajuste == 'KEEP':
            return fecha

        direccion = 1 if logica_ajuste == 'NEXT_WORKDAY' else -1
        fecha_temp = fecha
        
        # Buscamos el día hábil más cercano (Límite 30 iteraciones por seguridad)
        for _ in range(30):
            fecha_temp += timedelta(days=direccion)
            if self._es_habil(fecha_temp):
                return fecha_temp
        
        return fecha # Fallback

    def generar_calendario(self, regla_json: dict, anio: int) -> list:
        """
        EL CORAZÓN DEL SISTEMA:
        Recibe un JSON con CUALQUIER configuración y devuelve las fechas reales.
        
        Ejemplo JSON entrada:
        {
            "freq": "MONTHLY",      <- Frecuencia base
            "interval": 2,          <- Cada 2 meses
            "bymonthday": 15,       <- El día 15
            "ajuste": "NEXT_WORKDAY" <- Si cae festivo, mover al siguiente
        }
        """
        # 1. Configurar Parametros del Generador (RRULE)
        try:
            parametros_rrule = {
                "freq": FRECUENCIAS[regla_json.get("freq", "MONTHLY")],
                "dtstart": datetime(anio, 1, 1),
                "count": 366, # Límite de seguridad, se cortará por año
                "interval": regla_json.get("interval", 1)
            }
            
            # Agregar parámetros opcionales dinámicamente
            if "bymonthday" in regla_json:
                parametros_rrule["bymonthday"] = regla_json["bymonthday"]
            
            if "byweekday" in regla_json:
                # Convierte "MO,WE" a objetos [MO, WE]
                dias = [DIAS_SEMANA[d] for d in regla_json["byweekday"].split(",")]
                parametros_rrule["byweekday"] = dias
                
            if "bysetpos" in regla_json:
                # Ej: 1 (Primer lunes), -1 (Último viernes)
                parametros_rrule["bysetpos"] = regla_json["bysetpos"]

            # 2. Generar Fechas "Brutas" (Teóricas)
            fechas_teoricas = list(rrule(**parametros_rrule))
            
            # Filtramos solo las del año solicitado
            fechas_teoricas = [f.date() for f in fechas_teoricas if f.year == anio]

        except Exception as e:
            return {"error": f"Configuración de regla inválida: {str(e)}"}

        # 3. Procesar Ajustes y Semáforos
        calendario_final = []
        ajuste = regla_json.get("ajuste", "KEEP") # Por defecto no mueve

        for f_teorica in fechas_teoricas:
            f_real = self._ajustar_fecha(f_teorica, ajuste)
            
            calendario_final.append({
                "fecha_teorica": f_teorica,
                "es_festivo_original": (f_teorica in self.festivos or f_teorica.weekday() >= 5),
                "fecha_limite_real": f_real,
                "accion_tomada": "Ajustada por festivo" if f_real != f_teorica else "Sin cambios"
            })

        return calendario_final