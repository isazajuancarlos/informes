import csv
import io
from datetime import datetime
from app.core.database import get_db_session
from app.models.meta_tables import MetaVariable, Usuario, Informe, ConfiguracionInformeVariable
import re

class CsvLogic:
    
    @staticmethod
    def generar_plantilla():
        """Genera el CSV con las columnas dinámicas basadas en las variables activas"""
        with get_db_session() as db:
            # 1. Columnas Fijas (Estructura del Informe)
            headers = [
                'NOMBRE_INFORME (Texto)',
                'DESCRIPCION (Texto)',
                'RESPONSABLES (Emails separados por ;)',
                'FRECUENCIA (DAILY/MONTHLY/ONCE)',
                'INTERVALO (Numero Dias)',
                'FECHA_INICIO (YYYY-MM-DD)',
                'EXTENSIONES (pdf;xlsx;zip)'
            ]
            
            # 2. Columnas Dinámicas (Variables del Sistema)
            variables = db.query(MetaVariable).all()
            for v in variables:
                # Formato: VAR_{nombre_interno} ({TIPO})
                headers.append(f"VAR_{v.nombre_interno} ({v.tipo_dato})")

            # Crear CSV en memoria
            output = io.StringIO()
            writer = csv.writer(output)
            writer.writerow(headers)
            
            # Fila de ejemplo (opcional, ayuda al usuario)
            example = ['Reporte Caja', 'Cierre diario', 'admin@co.co;pepe@co.co', 'DAILY', '1', '2025-01-01', 'pdf;xlsx']
            example += ['' for _ in variables] # Rellenar variables vacías
            writer.writerow(example)
            
            return output.getvalue()

    @staticmethod
    def procesar_archivo(content_bytes):
        """
        Lee, valida y guarda. Retorna un reporte detallado de errores/éxitos.
        """
        reporte = {
            'total': 0,
            'exitos': 0,
            'errores': 0,
            'detalles': [] # Lista de {fila: 1, campo: 'nombre', estado: 'ERROR', msg: '...'}
        }

        # Decodificar
        try:
            content_str = content_bytes.decode('utf-8')
            f = io.StringIO(content_str)
            reader = csv.DictReader(f)
            fieldnames = reader.fieldnames
        except Exception as e:
            return {'error_critico': f"No se pudo leer el archivo CSV: {str(e)}"}

        with get_db_session() as db:
            # Pre-cargar datos para validar rápido
            usuarios_db = {u.email: u.id for u in db.query(Usuario).all()}
            vars_db = {v.nombre_interno: v for v in db.query(MetaVariable).all()}

            # Iterar filas
            for i, row in enumerate(reader, start=2): # Start 2 porque 1 es header
                reporte['total'] += 1
                errores_fila = []
                
                # --- VALIDACIÓN ESTRUCTURAL ---
                nombre = row.get('NOMBRE_INFORME (Texto)', '').strip()
                if not nombre: errores_fila.append("Nombre obligatorio")
                
                # Validar Frecuencia
                freq = row.get('FRECUENCIA (DAILY/MONTHLY/ONCE)', '').upper()
                if freq not in ['DAILY', 'MONTHLY', 'ONCE']: errores_fila.append(f"Frecuencia inválida: {freq}")
                
                # Validar Intervalo
                try:
                    intervalo = int(row.get('INTERVALO (Numero Dias)', 0))
                except:
                    errores_fila.append("Intervalo debe ser número entero")
                    intervalo = 0

                # Validar Fecha
                fecha_str = row.get('FECHA_INICIO (YYYY-MM-DD)', '')
                try:
                    datetime.strptime(fecha_str, '%Y-%m-%d')
                except:
                    errores_fila.append("Formato fecha inválido (use YYYY-MM-DD)")

                # Validar Responsables
                emails = row.get('RESPONSABLES (Emails separados por ;)', '').split(';')
                ids_responsables = []
                for email in emails:
                    email = email.strip()
                    if email and email in usuarios_db:
                        ids_responsables.append(usuarios_db[email])
                    elif email:
                        errores_fila.append(f"Usuario no existe: {email}")

                if not ids_responsables:
                    errores_fila.append("Debe asignar al menos un responsable válido")

                # --- VALIDACIÓN VARIABLES DINÁMICAS ---
                configs_vars = []
                for col in fieldnames:
                    if col.startswith("VAR_"):
                        # Extraer nombre limpio: "VAR_hora_entrega (NUMERO)" -> "hora_entrega"
                        nombre_var = col.split(' ')[0].replace("VAR_", "")
                        valor = row.get(col, '').strip()
                        
                        if valor and nombre_var in vars_db:
                            meta_var = vars_db[nombre_var]
                            
                            # Validar Tipo Estricto
                            es_valido = True
                            msg = ""
                            
                            if meta_var.tipo_dato == 'NUMERO' or meta_var.tipo_dato == 'MONEDA':
                                if not re.match(r'^\d+(\.\d+)?$', valor): es_valido = False; msg = "No es un número"
                            elif meta_var.tipo_dato == 'HORA_MILITAR':
                                if not re.match(r'^(?:[01]\d|2[0-3])[0-5]\d$', valor): es_valido = False; msg = "Formato hora inválido (HHMM)"
                            elif meta_var.tipo_dato == 'BOOLEANO':
                                if valor.lower() not in ['true', 'false', '1', '0', 'si', 'no']: es_valido = False; msg = "Use True/False"
                            
                            if not es_valido:
                                errores_fila.append(f"Var '{nombre_var}': {msg}")
                            else:
                                configs_vars.append((meta_var.id, valor))

                # --- DECISIÓN FILA ---
                if errores_fila:
                    reporte['errores'] += 1
                    for err in errores_fila:
                        reporte['detalles'].append({'fila': i, 'campo': 'General', 'estado': '❌ ERROR', 'msg': err})
                else:
                    # --- GUARDADO ---
                    try:
                        # 1. Crear Informe
                        recurrencia = {
                            "modo": "CICLICO", 
                            "frecuencia": freq, 
                            "intervalo": intervalo, 
                            "fecha_inicio": fecha_str
                        }
                        
                        exts = row.get('EXTENSIONES (pdf;xlsx;zip)', '').split(';')
                        datos_gen = {"extensiones": [e.strip() for e in exts if e.strip()]}
                        
                        nuevo_inf = Informe(
                            nombre=nombre,
                            descripcion=row.get('DESCRIPCION (Texto)', ''),
                            creador_id=1, # Admin por defecto
                            recurrencia_json=recurrencia,
                            datos_generales=datos_gen,
                            activo=True,
                            tipo_novedad="Creación (Carga Masiva)"
                        )
                        db.add(nuevo_inf)
                        db.flush()
                        
                        # 2. Asignar Usuarios
                        for uid in ids_responsables:
                            user_obj = db.get(Usuario, uid)
                            nuevo_inf.responsables.append(user_obj)
                            
                        # 3. Asignar Variables
                        for vid, vval in configs_vars:
                            db.add(ConfiguracionInformeVariable(
                                informe_id=nuevo_inf.id,
                                variable_id=vid,
                                valor_parametro=vval
                            ))
                            
                        db.commit()
                        reporte['exitos'] += 1
                        reporte['detalles'].append({'fila': i, 'campo': 'Todo', 'estado': '✅ OK', 'msg': f"Informe '{nombre}' creado"})
                        
                    except Exception as ex:
                        db.rollback()
                        reporte['errores'] += 1
                        reporte['detalles'].append({'fila': i, 'campo': 'BD', 'estado': '❌ CRÍTICO', 'msg': str(ex)})

        return reporte