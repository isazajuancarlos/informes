import os
import shutil
from fastapi import UploadFile, HTTPException
from datetime import datetime
from typing import List

class GestorArchivos:
    def __init__(self, directorio_base: str = "storage/evidencias"):
        # Crea la carpeta base si no existe
        self.base_dir = directorio_base
        os.makedirs(self.base_dir, exist_ok=True)

    def validar_formato(self, archivo: UploadFile, formatos_permitidos: List[str]):
        """
        Verifica si el archivo cumple con la regla de consecuencia.
        Ej: Si la regla dice ['pdf'], rechaza un .docx
        """
        if not formatos_permitidos:
            return True # Si la lista está vacía, permitimos todo (o nada, según política)
            
        ext = archivo.filename.split('.')[-1].lower()
        if ext not in [f.lower() for f in formatos_permitidos]:
            raise HTTPException(
                status_code=400, 
                detail=f"Formato inválido. Se requiere: {formatos_permitidos}. Recibido: {ext}"
            )

    def construir_ruta_dinamica(self, metadatos_informe: dict) -> str:
        """
        Crea una ruta basada en los N parámetros del informe.
        Ej: /storage/2025/Riesgo_Alto/Informe_105/
        """
        # Obtenemos valores o defaults
        anio = str(datetime.now().year)
        
        # Intentamos sacar datos clave del JSON del informe (si existen)
        dependencia = metadatos_informe.get("dependencia", "General")
        categoria = metadatos_informe.get("categoria", "Varios")
        
        # Limpiamos strings para evitar carpetas con espacios raros
        dependencia = "".join([c for c in dependencia if c.isalnum()])
        categoria = "".join([c for c in categoria if c.isalnum()])
        
        ruta_relativa = os.path.join(anio, dependencia, categoria)
        ruta_absoluta = os.path.join(self.base_dir, ruta_relativa)
        
        # Crear carpetas físicas
        os.makedirs(ruta_absoluta, exist_ok=True)
        
        return ruta_absoluta

    async def guardar_evidencia(self, archivo: UploadFile, informe_id: int, metadatos: dict) -> str:
        # 1. Definir ruta
        carpeta_destino = self.construir_ruta_dinamica(metadatos)
        
        # 2. Definir nombre estandarizado (Ej: INF_105_evidencia.pdf)
        # Esto evita que se pierdan archivos si el usuario los sube con nombres raros
        ext = archivo.filename.split('.')[-1]
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        nombre_final = f"INF_{informe_id}_{timestamp}.{ext}"
        
        ruta_completa = os.path.join(carpeta_destino, nombre_final)
        
        # 3. Escribir en disco
        with open(ruta_completa, "wb") as buffer:
            shutil.copyfileobj(archivo.file, buffer)
            
        return ruta_completa