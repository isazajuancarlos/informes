from typing import List, Dict, Any
from app.schemas.dynamic_core import MetaReglaCreate, OperadorLogicoEnum, TipoConsecuenciaEnum

class MotorCausalidad:
    def evaluar_impacto(self, valores_usuario: Dict[str, Any], reglas: List[MetaReglaCreate]) -> Dict[str, Any]:
        plan = {
            "alertas_generadas": [],
            "archivos_requeridos": [],
            "roles_a_notificar": [],
            "ajuste_fecha_dias": 0,      # Para DATE_CHANGE: días a sumar/restar al límite
            "proceso_bloqueado": False,  # Para BLOCK: impide que el usuario continúe
        }

        for regla in reglas:
            # 1. Obtener valor del usuario
            valor_real = valores_usuario.get(regla.variable_trigger)
            
            # Si el usuario no llenó el campo, saltamos
            if valor_real is None or valor_real == "": 
                continue

            # 2. Evaluar Condición
            se_cumple = self._evaluar(valor_real, regla.operador, regla.valor_comparacion)
            
            if se_cumple:
                self._aplicar(plan, regla)

        return plan

    def _evaluar(self, real, op, ref):
        """
        Compara valores de forma inteligente (Numérica o Texto).
        """
        # --- CORRECCIÓN CRÍTICA: Protección contra Nulos ---
        # Si la regla no tiene valor de referencia configurado (es None), 
        # no se puede evaluar, así que asumimos Falso.
        if ref is None:
            return False
        # ---------------------------------------------------

        try:
            # A. INTENTO NUMÉRICO
            try:
                v_real = float(real)
                v_ref = float(ref)
                
                if op == OperadorLogicoEnum.IGUAL: return v_real == v_ref
                if op == OperadorLogicoEnum.MAYOR_QUE: return v_real > v_ref
                if op == OperadorLogicoEnum.MENOR_QUE: return v_real < v_ref
                if op == OperadorLogicoEnum.MAYOR_O_IGUAL: return v_real >= v_ref
                if op == OperadorLogicoEnum.MENOR_O_IGUAL: return v_real <= v_ref
                
            except ValueError:
                # Si falla la conversión a número, seguimos con texto
                pass

            # B. LÓGICA DE TEXTO
            s_real = str(real).strip().lower()
            s_ref = str(ref).strip().lower()

            if op == OperadorLogicoEnum.IGUAL: return s_real == s_ref
            if op == OperadorLogicoEnum.CONTIENE: return s_ref in s_real

            # C. IN_LIST: verifica si el valor está dentro de una lista de opciones
            if op == OperadorLogicoEnum.DENTRO_DE:
                if isinstance(ref, list):
                    return s_real in [str(v).strip().lower() for v in ref]
                # Si ref es string con comas ("A,B,C"), lo tratamos como lista
                if isinstance(ref, str) and "," in ref:
                    opciones = [v.strip().lower() for v in ref.split(",")]
                    return s_real in opciones
                return s_real == s_ref

        except Exception as e:
            print(f"⚠️ Error silencioso evaluando regla: {e}")
            return False

        return False

    def _aplicar(self, plan, regla):
        pl = regla.payload

        # Mensaje de alerta siempre se agrega si existe (independiente del tipo de consecuencia)
        if pl.mensaje_alerta:
            plan["alertas_generadas"].append(pl.mensaje_alerta)

        if regla.tipo_consecuencia == TipoConsecuenciaEnum.SOLICITAR_ARCHIVO and pl.formatos_permitidos:
            plan["archivos_requeridos"].extend(pl.formatos_permitidos)

        if regla.tipo_consecuencia == TipoConsecuenciaEnum.NOTIFICAR_ROL and pl.rol_destino:
            plan["roles_a_notificar"].append(pl.rol_destino)

        if regla.tipo_consecuencia == TipoConsecuenciaEnum.CAMBIAR_FECHA and pl.dias_ajuste is not None:
            # Acumula ajuste de días (puede ser positivo o negativo)
            plan["ajuste_fecha_dias"] = plan.get("ajuste_fecha_dias", 0) + pl.dias_ajuste

        if regla.tipo_consecuencia == TipoConsecuenciaEnum.BLOQUEAR_PROCESO:
            # Bloquea el proceso; el mensaje de alerta ya fue agregado arriba
            plan["proceso_bloqueado"] = True