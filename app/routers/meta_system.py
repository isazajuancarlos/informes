from fastapi import APIRouter, HTTPException, Depends
from sqlalchemy.orm import Session
from typing import List, Dict, Any

# Importamos la conexión
from app.core.database import get_db
# Importamos los Modelos (Tablas)
from app.models.meta_tables import MetaVariable, MetaRegla
# Importamos los Schemas (Validación)
from app.schemas.dynamic_core import (
    MetaVariableCreate, 
    MetaReglaCreate, 
    InformeDinamicoIngreso
)
from app.services.causality_engine import MotorCausalidad

router = APIRouter(
    prefix="/api/v1/meta",
    tags=["Meta-Sistema (Persistencia Real)"]
)

motor = MotorCausalidad()

# ==========================================
# 1. GESTIÓN DE VARIABLES (Inputs)
# ==========================================

@router.post("/variables", response_model=MetaVariableCreate)
def crear_nueva_variable(variable: MetaVariableCreate, db: Session = Depends(get_db)):
    # 1. Verificar si ya existe en PostgreSQL
    existe = db.query(MetaVariable).filter(MetaVariable.nombre_interno == variable.nombre_interno).first()
    if existe:
        raise HTTPException(status_code=400, detail="Esta variable ya existe en la base de datos.")

    # 2. Crear objeto de Base de Datos
    nueva_var_bd = MetaVariable(
        nombre_interno=variable.nombre_interno,
        etiqueta_visual=variable.etiqueta_visual,
        tipo_dato=variable.tipo_dato,
        es_obligatorio=variable.es_obligatorio,
        # Convertimos la lista de Pydantic a JSON puro para PostgreSQL
        opciones_json=[op.dict() for op in variable.opciones] if variable.opciones else []
    )

    # 3. Guardar
    db.add(nueva_var_bd)
    db.commit()
    db.refresh(nueva_var_bd)
    
    # Retornamos lo guardado (reconvertimos el JSON a objeto para que el Schema no falle)
    variable.opciones = nueva_var_bd.opciones_json
    return variable

@router.get("/variables", response_model=List[MetaVariableCreate])
def listar_variables(db: Session = Depends(get_db)):
    # SELECT * FROM meta_variables
    variables_bd = db.query(MetaVariable).all()
    
    # Mapeo manual simple para ajustar el campo JSON a Pydantic
    resultado = []
    for v in variables_bd:
        item = MetaVariableCreate(
            nombre_interno=v.nombre_interno,
            etiqueta_visual=v.etiqueta_visual,
            tipo_dato=v.tipo_dato,
            es_obligatorio=v.es_obligatorio,
            opciones=v.opciones_json
        )
        resultado.append(item)
    return resultado

# ==========================================
# 2. GESTIÓN DE REGLAS (Lógica)
# ==========================================

@router.post("/reglas", response_model=MetaReglaCreate)
def crear_nueva_regla(regla: MetaReglaCreate, db: Session = Depends(get_db)):
    # 1. Verificar que la variable trigger exista
    var_db = db.query(MetaVariable).filter(MetaVariable.nombre_interno == regla.variable_trigger).first()
    if not var_db:
        raise HTTPException(status_code=404, detail=f"La variable '{regla.variable_trigger}' no existe.")

    # 2. Crear objeto BD
    nueva_regla_bd = MetaRegla(
        nombre_regla=regla.nombre_regla,
        variable_id=var_db.id, # Usamos el ID relacional
        operador=regla.operador,
        valor_comparacion_json=regla.valor_comparacion,
        tipo_consecuencia=regla.tipo_consecuencia,
        payload_json=regla.payload.dict()
    )

    db.add(nueva_regla_bd)
    db.commit()
    db.refresh(nueva_regla_bd)
    
    return regla

# ==========================================
# 3. SIMULACIÓN CON DATOS REALES
# ==========================================

@router.post("/simular-informe", response_model=Dict[str, Any])
def simular_comportamiento(datos: InformeDinamicoIngreso, db: Session = Depends(get_db)):
    """
    Ahora esta simulación lee las reglas VIVAS de la base de datos.
    """
    # 1. Traer todas las reglas guardadas
    # Nota: En producción, filtraríamos solo las reglas activas del informe específico
    reglas_bd = db.query(MetaRegla).all()
    
    # 2. Convertir de SQL-Alchemy a objetos Pydantic que el motor entienda
    reglas_convertidas = []
    for r in reglas_bd:
        # Necesitamos el nombre de la variable, no el ID. Hacemos un join implícito.
        # Gracias a la relación en el modelo: r.variable.nombre_interno
        objeto_regla = MetaReglaCreate(
            nombre_regla=r.nombre_regla,
            variable_trigger=r.variable.nombre_interno,
            operador=r.operador,
            valor_comparacion=r.valor_comparacion_json,
            tipo_consecuencia=r.tipo_consecuencia,
            payload=r.payload_json
        )
        reglas_convertidas.append(objeto_regla)

    # 3. El Motor evalúa
    resultado = motor.evaluar_impacto(datos.valores_usuario, reglas_convertidas)
    
    return resultado