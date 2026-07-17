from pydantic import BaseModel, Field
from typing import List, Optional, Any, Dict
from enum import Enum

# ==========================================
# 1. ENUMS (El Vocabulario Controlado)
# ==========================================

class TipoDatoEnum(str, Enum):
    TEXTO = "TEXTO"
    NUMERO = "NUMERO"
    FECHA = "FECHA"
    BOOLEANO = "BOOLEANO" 
    LISTA_DESPLEGABLE = "LISTA"
    HORA_MILITAR = "HORA_MILITAR" # Nuevo
    MONEDA = "MONEDA" # Nuevo
    TEXTO_ALFANUM = "TEXTO_ALFANUM" # Nuevo

class OperadorLogicoEnum(str, Enum):
    IGUAL = "EQUALS"
    MAYOR_QUE = "GREATER_THAN"
    MENOR_QUE = "LESS_THAN"
    
    # --- NUEVOS OPERADORES AGREGADOS ---
    MAYOR_O_IGUAL = "GREATER_THAN_OR_EQUAL"
    MENOR_O_IGUAL = "LESS_THAN_OR_EQUAL"
    # -----------------------------------
    
    CONTIENE = "CONTAINS" 
    DENTRO_DE = "IN_LIST" 

class TipoConsecuenciaEnum(str, Enum):
    SOLICITAR_ARCHIVO = "FILE_REQ"    
    CAMBIAR_FECHA = "DATE_CHANGE"     
    NOTIFICAR_ROL = "NOTIFY_ROLE"     
    BLOQUEAR_PROCESO = "BLOCK"        

# ==========================================
# 2. DEFINICIÓN DE ESQUEMAS
# ==========================================

class ConsecuenciaPayload(BaseModel):
    mensaje_alerta: Optional[str] = None
    formatos_permitidos: Optional[List[str]] = None 
    dias_ajuste: Optional[int] = None 
    rol_destino: Optional[str] = None 

class MetaReglaCreate(BaseModel):
    nombre_regla: str
    variable_trigger: str 
    
    # Ahora aceptará LESS_THAN_OR_EQUAL
    operador: OperadorLogicoEnum 
    
    valor_comparacion: Any 
    
    tipo_consecuencia: TipoConsecuenciaEnum
    payload: ConsecuenciaPayload