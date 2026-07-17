# backend/app/routers/informes.py
from fastapi import BackgroundTasks

@app.post("/guardar-informe")
async def guardar_informe(datos: InformeSchema, background_tasks: BackgroundTasks):
    # 1. Guardar Datos
    informe = db.save(datos)
    
    # 2. Correr Motor de Causalidad
    efectos = motor.procesar_causalidad(datos, reglas_activas)
    
    # 3. Disparar Consecuencias (Notificaciones)
    if efectos['notificaciones']:
        for notif in efectos['notificaciones']:
            background_tasks.add_task(
                enviar_correo_seguro, 
                destinatario=notif['rol_destino'], 
                asunto="Actualización de Sistema",
                cuerpo=generar_plantilla(notif['plantilla'], datos)
            )
            
    return {"status": "Guardado", "consecuencias": efectos}