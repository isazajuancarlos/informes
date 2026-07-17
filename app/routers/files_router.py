from fastapi import APIRouter, UploadFile, File, Form, Depends, HTTPException
from sqlalchemy.orm import Session
from app.core.database import get_db
from app.models.meta_tables import InformeDinamico
from app.services.file_manager import GestorArchivos
import json

router = APIRouter(
    prefix="/api/v1/files",
    tags=["Gestión de Evidencias"]
)

gestor = GestorArchivos()

@router.post("/upload/{informe_id}")
async def subir_evidencia(
    informe_id: int,
    archivo: UploadFile = File(...),
    db: Session = Depends(get_db)
):
    """
    Sube un archivo, pero SOLO si cumple con las reglas dinámicas del informe.
    """
    # 1. Buscar el informe en BD
    informe = db.query(InformeDinamico).filter(InformeDinamico.id == informe_id).first()
    if not informe:
        raise HTTPException(status_code=404, detail="Informe no encontrado")

    # 2. Analizar las Consecuencias Activas (Lógica Consecuencial)
    # Buscamos si en 'consecuencias_aplicadas' hay requisitos de archivo
    requisitos = informe.consecuencias_aplicadas or {}
    formatos_obligatorios = requisitos.get("archivos_requeridos", []) # Ej: ['pdf']
    
    # 3. Validar Formato (El sistema rechaza si no cumple la regla)
    gestor.validar_formato(archivo, formatos_obligatorios)

    # 4. Guardar Archivo
    # Usamos los datos del informe para crear la carpeta
    ruta_guardada = await gestor.guardar_evidencia(
        archivo, 
        informe_id, 
        informe.datos_valores # Pasamos los parámetros (N) para crear la ruta
    )

    # 5. Actualizar la BD con la ruta de la evidencia
    # Actualizamos el JSON del informe agregando la evidencia
    datos_actuales = dict(informe.datos_valores) # Copia mutable
    if "evidencias" not in datos_actuales:
        datos_actuales["evidencias"] = []
    
    datos_actuales["evidencias"].append({
        "ruta": ruta_guardada,
        "nombre_original": archivo.filename,
        "fecha_subida": str(datetime.now())
    })
    
    informe.datos_valores = datos_actuales
    db.commit()

    return {
        "estado": "Exitoso", 
        "mensaje": "Evidencia guardada y validada según reglas.",
        "ruta_interna": ruta_guardada
    }