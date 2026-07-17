from fastapi import APIRouter
from pydantic import BaseModel
from datetime import date
from app.services.calendar_engine import MotorCalendario

router = APIRouter()
motor = MotorCalendario(pais_iso='CO') # Configurado para Colombia

class SolicitudCalculo(BaseModel):
    fecha_base: date # Ej: 2024-12-01
    regla: dict # Ej: {"freq": "MENSUAL", "dia_objetivo": 5, "ajuste": ">"}

@router.post("/simular-fecha")
def simular_vencimiento(datos: SolicitudCalculo):
    resultado = motor.calcular_vencimiento_regla(datos.fecha_base, datos.regla)
    return {
        "entrada": datos,
        "calculo": resultado,
        "mensaje": "Cálculo exitoso usando festivos de Colombia"
    }

@router.get("/es-festivo/{fecha}")
def verificar_festivo(fecha: date):
    es_festivo = fecha in motor.festivos
    nombre_festivo = motor.festivos.get(fecha) # Ej: "Día de la Independencia"
    return {
        "fecha": fecha,
        "es_festivo": es_festivo,
        "nombre": nombre_festivo,
        "es_dia_habil": motor.es_dia_habil(fecha)
    }