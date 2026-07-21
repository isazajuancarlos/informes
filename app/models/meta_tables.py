from sqlalchemy import (
    Column, Integer, String, Boolean, ForeignKey, JSON, DateTime, Table,
    UniqueConstraint, CheckConstraint, Index, Text, text,
)
from sqlalchemy.orm import relationship
from datetime import datetime
from app.core.database import Base

# =====================================================
# VOCABULARIO DEL FLUJO
# =====================================================
# El flujo es lo único invariante del producto: quién produce la información,
# quién la recibe y quién la consume. Todo lo demás — campos, reglas, variables,
# taxonomías — es configuración de cada cliente y NO se modela con columnas.
#
# Estos cinco roles son deliberadamente genéricos: sirven igual a un reporte
# regulatorio, a un entregable contractual o a un informe interno de gerencia.
ROL_ORIGEN = "ORIGEN"        # produce y envía la información
ROL_DESTINO = "DESTINO"      # la recibe
ROL_USUARIO = "USUARIO"      # la consume (puede no ser quien la recibe)
ROL_SUPERVISA = "SUPERVISA"  # responde por el cumplimiento
ROL_EJECUTA = "EJECUTA"      # hace el trabajo material

ROLES_FLUJO = (ROL_ORIGEN, ROL_DESTINO, ROL_USUARIO, ROL_SUPERVISA, ROL_EJECUTA)

# =====================================================
# 0. ESTRUCTURA ORGANIZACIONAL (v4.0)
# =====================================================

class UnidadOrganizacional(Base):
    __tablename__ = "unidades_organizacionales"
    id = Column(Integer, primary_key=True, index=True)
    nombre = Column(String(200), nullable=False)
    codigo = Column(String(50), unique=True, nullable=True)
    descripcion = Column(Text, nullable=True)
    padre_id = Column(Integer, ForeignKey("unidades_organizacionales.id"), nullable=True)
    nivel = Column(Integer, default=1)
    activo = Column(Boolean, default=True)
    padre = relationship("UnidadOrganizacional", remote_side="UnidadOrganizacional.id", backref="hijos")

class ConfiguracionSistema(Base):
    __tablename__ = "configuracion_sistema"
    clave = Column(String(100), primary_key=True)
    valor = Column(Text, nullable=True)
    descripcion = Column(String(300), nullable=True)

# =====================================================
# 1. SEGURIDAD Y USUARIOS
# =====================================================

class Usuario(Base):
    __tablename__ = "usuarios"
    id = Column(Integer, primary_key=True, index=True)
    nombre = Column(String(100), nullable=False)
    email = Column(String(100), unique=True, nullable=False, index=True)
    password_hash = Column(String(255), nullable=False)
    es_admin = Column(Boolean, default=False)
    
    # NUEVO CAMPO: ROL EXPLÍCITO
    # Opciones: 'ADMIN', 'OPERADOR', 'SUPERVISOR', 'TERCERO'
    rol_sistema = Column(String(50), default='OPERADOR') 
    
    activo = Column(Boolean, default=True)

    # v4.0: Unidad organizacional
    unidad_id = Column(Integer, ForeignKey("unidades_organizacionales.id"), nullable=True)
    unidad = relationship("UnidadOrganizacional", foreign_keys=[unidad_id])

    entregas = relationship("EntregaEsperada", back_populates="usuario", foreign_keys="[EntregaEsperada.usuario_id]")
    stakeholder_perfil = relationship("Stakeholder", back_populates="usuario_acceso", uselist=False)
    participaciones = relationship(
        "Participacion", back_populates="usuario", cascade="all, delete-orphan"
    )

    @property
    def informes_asignados(self):
        return [p.informe for p in self.participaciones]

    # Relación con sus reportes personalizados
    reportes_analiticos = relationship("ReporteAnalitico", back_populates="creador")
    notificaciones = relationship("NotificacionApp", back_populates="usuario")

class Delegacion(Base):
    __tablename__ = "delegaciones"
    id = Column(Integer, primary_key=True, index=True)
    delegante_id = Column(Integer, ForeignKey("usuarios.id"), nullable=False)
    delegado_id = Column(Integer, ForeignKey("usuarios.id"), nullable=False)
    fecha_inicio = Column(DateTime, nullable=False)
    fecha_fin = Column(DateTime, nullable=False)
    motivo = Column(Text, nullable=True)
    activo = Column(Boolean, default=True)
    delegante = relationship("Usuario", foreign_keys=[delegante_id])
    delegado = relationship("Usuario", foreign_keys=[delegado_id])

class NotificacionApp(Base):
    __tablename__ = "notificaciones_app"
    id = Column(Integer, primary_key=True, index=True)
    usuario_id = Column(Integer, ForeignKey("usuarios.id"), nullable=False)
    tipo = Column(String(20), nullable=False)  # INFO, ALERTA, URGENTE
    titulo = Column(String(200), nullable=False)
    mensaje = Column(Text, nullable=False)
    leida = Column(Boolean, default=False)
    fecha_creacion = Column(DateTime, default=datetime.utcnow)
    url_destino = Column(String(500), nullable=True)
    entidad_tipo = Column(String(50), nullable=True)
    entidad_id = Column(Integer, nullable=True)
    usuario = relationship("Usuario", back_populates="notificaciones")

class ReporteAnalitico(Base):
    """
    Guarda la configuración de un gráfico creado por el Supervisor.
    No guarda datos, guarda la 'pregunta'.
    """
    __tablename__ = "reportes_analiticos"
    id = Column(Integer, primary_key=True, index=True)
    usuario_id = Column(Integer, ForeignKey("usuarios.id"))
    nombre = Column(String(100)) # Ej: "Comparativa Mensual"
    
    # Configuración del gráfico (Filtros y Tipo)
    configuracion = Column(JSON) 
    # Ej: { "tipo_grafico": "BAR", "filtro_fecha": "2025", "metricas": ["CUMPLIDO"] }

    creador = relationship("Usuario", back_populates="reportes_analiticos")

# =====================================================
# 2. TERCEROS (STAKEHOLDERS)
# =====================================================

class Stakeholder(Base):
    __tablename__ = "stakeholders"
    id = Column(Integer, primary_key=True, index=True)
    nombre = Column(String(200), nullable=False)
    contacto_nombre = Column(String(200))
    email = Column(String(150))
    tipo = Column(String(50)) 
    usuario_id = Column(Integer, ForeignKey("usuarios.id"), nullable=True)
    usuario_acceso = relationship("Usuario", back_populates="stakeholder_perfil")
    participaciones = relationship(
        "Participacion", back_populates="stakeholder", cascade="all, delete-orphan"
    )

    @property
    def informes_solicitados(self):
        return [p.informe for p in self.participaciones if p.rol == ROL_USUARIO]

    @property
    def informes_recibidos(self):
        return [p.informe for p in self.participaciones if p.rol == ROL_DESTINO]

class SolicitudNuevoInforme(Base):
    __tablename__ = "solicitudes_terceros"
    id = Column(Integer, primary_key=True, index=True)
    stakeholder_id = Column(Integer, ForeignKey("stakeholders.id"))
    titulo = Column(String(200))
    descripcion = Column(Text)
    fecha_solicitud = Column(DateTime, default=datetime.utcnow)
    estado = Column(String(50), default="PENDIENTE") 
    respuesta_admin = Column(Text, nullable=True)
    stakeholder = relationship("Stakeholder")

# =====================================================
# 3. CATÁLOGO DE VARIABLES Y REGLAS
# =====================================================

class MetaVariable(Base):
    __tablename__ = "meta_variables"
    id = Column(Integer, primary_key=True, index=True)
    nombre_interno = Column(String(50), unique=True, nullable=False)
    etiqueta_visual = Column(String(150), nullable=False)
    tipo_dato = Column(String(20), nullable=False) 
    es_obligatorio = Column(Boolean, default=True)
    descripcion = Column(String(255))
    reglas_base = relationship("MetaRegla", back_populates="variable", cascade="all, delete-orphan")

class MetaRegla(Base):
    __tablename__ = "meta_reglas_negocio"
    id = Column(Integer, primary_key=True, index=True)
    variable_id = Column(Integer, ForeignKey("meta_variables.id"))
    nombre_regla = Column(String(100))
    operador = Column(String(20))
    valor_comparacion_json = Column(JSON, nullable=True)
    tipo_consecuencia = Column(String(50))
    payload_json = Column(JSON) 
    variable = relationship("MetaVariable", back_populates="reglas_base")

# =====================================================
# 4. GESTIÓN DE INFORMES
# =====================================================

# NUEVO: Tabla para Conjuntos de Datos
class ConjuntoDatos(Base):
    """
    Catálogo de conjuntos de datos (ej: RIPS, RUAF, Vacunación, etc.)
    Cada conjunto tiene un responsable asignado.
    """
    __tablename__ = "conjuntos_datos"
    id = Column(Integer, primary_key=True, index=True)
    nombre = Column(String(200), unique=True, nullable=False, index=True)
    descripcion = Column(Text, nullable=True)
    responsable_id = Column(Integer, ForeignKey("usuarios.id"), nullable=True)
    activo = Column(Boolean, default=True)
    fecha_creacion = Column(DateTime, default=datetime.utcnow)

    responsable = relationship("Usuario")
    informes = relationship("Informe", back_populates="conjunto_datos")

class Participacion(Base):
    """Quién participa en el flujo de un informe, y con qué rol.

    ÚNICA fuente de verdad del flujo. Antes esto vivía en cuatro sitios:
    `asignaciones_informe_usuario` (partes internas, con el rol en una columna),
    `rel_informe_solicitantes` y `rel_informe_destinatarios` (partes externas,
    cuyo rol estaba implícito en CUÁL tabla puente las contenía), y `tipo_flujo`,
    un resumen almacenado que podía contradecir a las otras tres.

    Una parte es una parte: puede ser interna (`Usuario`, que entra al sistema)
    o externa (`Stakeholder`, que no), y eso es ortogonal a su papel en el
    flujo. Así un informe puede recibirse de un tercero y consumirse
    internamente sin que el modelo tenga que decidir de qué "tipo" es.
    """

    __tablename__ = "participacion_informe"

    id = Column(Integer, primary_key=True, index=True)
    informe_id = Column(Integer, ForeignKey("cat_informes.id"), nullable=False, index=True)

    # Exactamente una de las dos: interna o externa (lo garantiza el CHECK).
    usuario_id = Column(Integer, ForeignKey("usuarios.id"), nullable=True)
    stakeholder_id = Column(Integer, ForeignKey("stakeholders.id"), nullable=True)

    rol = Column(String(20), nullable=False, index=True)
    es_principal = Column(Boolean, default=True)   # principal vs. suplente
    orden = Column(Integer, default=0)

    informe = relationship("Informe", back_populates="participaciones")
    usuario = relationship("Usuario", back_populates="participaciones")
    stakeholder = relationship("Stakeholder", back_populates="participaciones")

    # OJO: un UniqueConstraint sobre (informe_id, usuario_id, stakeholder_id, rol)
    # NO restringe nada, porque una de las dos columnas de parte siempre es NULL
    # y en SQL NULL != NULL: dos filas idénticas se consideran distintas. Hacen
    # falta dos índices ÚNICOS PARCIALES, uno por naturaleza de parte.
    __table_args__ = (
        CheckConstraint(
            "(usuario_id IS NOT NULL AND stakeholder_id IS NULL) OR "
            "(usuario_id IS NULL AND stakeholder_id IS NOT NULL)",
            name="ck_participacion_una_sola_parte",
        ),
        Index(
            "uq_participacion_interna", "informe_id", "usuario_id", "rol",
            unique=True,
            sqlite_where=text("usuario_id IS NOT NULL"),
            postgresql_where=text("usuario_id IS NOT NULL"),
        ),
        Index(
            "uq_participacion_externa", "informe_id", "stakeholder_id", "rol",
            unique=True,
            sqlite_where=text("stakeholder_id IS NOT NULL"),
            postgresql_where=text("stakeholder_id IS NOT NULL"),
        ),
    )

    @property
    def es_externa(self) -> bool:
        return self.stakeholder_id is not None

    @property
    def parte(self):
        return self.stakeholder if self.es_externa else self.usuario

    @property
    def nombre(self) -> str:
        return getattr(self.parte, "nombre", "") if self.parte else ""


class Informe(Base):
    __tablename__ = "cat_informes"
    id = Column(Integer, primary_key=True, index=True)
    nombre = Column(String(200), unique=True, nullable=False)
    descripcion = Column(String(500))
    creador_id = Column(Integer, ForeignKey("usuarios.id"))
    fecha_creacion = Column(DateTime, default=datetime.utcnow)
    recurrencia_json = Column(JSON, default=dict)
    datos_generales = Column(JSON, default=dict)
    activo = Column(Boolean, default=True)
    tipo_novedad = Column(String(50), default="Creación")
    motivo_desactivacion = Column(String(500), nullable=True)
    fecha_actualizacion = Column(DateTime, nullable=True)
    fecha_desactivacion = Column(DateTime, nullable=True)

    # ===== NUEVOS CAMPOS (Fase 1 y 2) =====

    # `tipo_flujo` YA NO es una columna: se calcula a partir de las partes
    # (ver la propiedad más abajo). Almacenarlo permitía que un informe marcado
    # RECEPCION_EXTERNA tuviera todas sus partes internas.

    # Subsistema y Conjunto de Datos (antes en JSON, ahora relacionales)
    subsistema = Column(String(100), nullable=True, index=True)
    conjunto_datos_id = Column(Integer, ForeignKey("conjuntos_datos.id"), nullable=True)

    # Acuerdos de Confidencialidad (ALTA PRIORIDAD)
    tiene_acuerdo_confidencialidad = Column(Boolean, default=False)
    cargo_autorizador = Column(String(200), nullable=True)

    # Plantilla del Informe (ALTA PRIORIDAD)
    url_plantilla = Column(String(500), nullable=True)
    ruta_plantilla = Column(String(500), nullable=True)

    # Justificación y Normatividad (MEDIA PRIORIDAD)
    justificacion_normatividad = Column(Text, nullable=True)

    # Aplicativo/Herramienta Origen (MEDIA PRIORIDAD)
    aplicativo_origen = Column(String(200), nullable=True)
    criterios_generacion = Column(Text, nullable=True)

    # Rutas de Almacenamiento (MEDIA PRIORIDAD)
    ruta_envio = Column(String(500), nullable=True)
    ruta_respaldo = Column(String(500), nullable=True)

    # Fecha de Corte y Días Límite (información adicional)
    fecha_corte_descripcion = Column(String(200), nullable=True)  # Ej: "1 al 30"
    dias_limite_descripcion = Column(String(200), nullable=True)  # Ej: "Primeros 5 días"

    # v4.0: Unidad responsable
    unidad_id = Column(Integer, ForeignKey("unidades_organizacionales.id"), nullable=True)

    # ===== RELACIONES =====

    participaciones = relationship(
        "Participacion", back_populates="informe", cascade="all, delete-orphan"
    )
    configuracion_variables = relationship("ConfiguracionInformeVariable", back_populates="informe", cascade="all, delete-orphan")
    conjunto_datos = relationship("ConjuntoDatos", back_populates="informes")
    unidad = relationship("UnidadOrganizacional", foreign_keys=[unidad_id])

    # ===== EL FLUJO =====

    def partes(self, rol=None, externas=None):
        """Partes del flujo, filtrables por rol y por interna/externa."""
        ps = self.participaciones
        if rol is not None:
            ps = [p for p in ps if p.rol == rol]
        if externas is not None:
            ps = [p for p in ps if p.es_externa is externas]
        return sorted(ps, key=lambda p: (not p.es_principal, p.orden))

    @property
    def responsables(self):
        """Personas internas que ejecutan o envían (lo que antes era la
        asignación por defecto)."""
        return [p.usuario for p in self.partes() if not p.es_externa
                and p.rol in (ROL_EJECUTA, ROL_ORIGEN)]

    @property
    def solicitantes(self):
        return [p.stakeholder for p in self.partes(ROL_USUARIO, externas=True)]

    @property
    def destinatarios(self):
        return [p.stakeholder for p in self.partes(ROL_DESTINO, externas=True)]

    @property
    def supervisores(self):
        """Quienes responden por el cumplimiento. Son el destinatario de
        último recurso de cualquier aviso: si nadie tiene asignada una
        entrega, el aviso tiene que llegarle a alguien."""
        return [p.usuario for p in self.partes(ROL_SUPERVISA) if not p.es_externa]

    @property
    def tiene_quien_la_reciba(self) -> bool:
        """Un informe necesita a alguien que lo reciba O que lo consulte.

        No son lo mismo: un tablero preparado para que un auditor lo consulte
        no se le ENVÍA a nadie, y aun así la obligación existe. Exigir DESTINO
        marcaba ese caso legítimo como defectuoso.
        """
        return bool(self.partes(ROL_DESTINO) or self.partes(ROL_USUARIO))

    @property
    def tipo_flujo(self) -> str:
        """Derivado, nunca almacenado: lo dicen las partes.

        Sale información si algún DESTINO es externo; entra si algún ORIGEN lo
        es. Ambas cosas a la vez es un flujo bidireccional; ninguna, interno.
        """
        sale = any(p.es_externa for p in self.participaciones if p.rol == ROL_DESTINO)
        entra = any(p.es_externa for p in self.participaciones if p.rol == ROL_ORIGEN)
        if sale and entra:
            return "BIDIRECCIONAL"
        if sale:
            return "ENVIO_EXTERNO"
        if entra:
            return "RECEPCION_EXTERNA"
        return "ENVIO_INTERNO"

class ConfiguracionInformeVariable(Base):
    __tablename__ = "config_informe_variables"
    id = Column(Integer, primary_key=True, index=True)
    informe_id = Column(Integer, ForeignKey("cat_informes.id"))
    variable_id = Column(Integer, ForeignKey("meta_variables.id"))
    valor_parametro = Column(String(255), nullable=True) 
    informe = relationship("Informe", back_populates="configuracion_variables")
    variable = relationship("MetaVariable")
    __table_args__ = (UniqueConstraint('informe_id', 'variable_id', name='unique_inf_var'),)

# =====================================================
# 5. EJECUCIÓN
# =====================================================

class EntregaEsperada(Base):
    __tablename__ = "entregas_esperadas"
    id = Column(Integer, primary_key=True, index=True)
    informe_id = Column(Integer, ForeignKey("cat_informes.id"))
    usuario_id = Column(Integer, ForeignKey("usuarios.id"), nullable=True)
    fecha_programada = Column(DateTime, default=datetime.utcnow)
    fecha_limite_teorica = Column(DateTime)
    fecha_entrega_real = Column(DateTime, nullable=True)
    estado = Column(String(20), default="PENDIENTE")
    ruta_archivo = Column(String(500), nullable=True)

    # NUEVO: Indicador de creación manual (A Demanda)
    es_manual = Column(Boolean, default=False)
    # Si es True, fue creada manualmente por admin (no por scheduler automático)

    creador_manual_id = Column(Integer, ForeignKey("usuarios.id"), nullable=True)
    # Usuario que creó manualmente esta entrega (si aplica)

    notas_admin = Column(Text, nullable=True)
    # Notas adicionales del admin al crear entrega manual

    # Cuándo se avisó de cada cosa. Sin esto el servicio de alertas reenviaba
    # el mismo aviso en CADA corrida —cada 6 h, indefinidamente— porque su
    # única condición era "vencida y pendiente", que no deja de cumplirse.
    # Avisar de más entrena a la gente a ignorar el sistema, que es peor que
    # no avisar.
    alerta_proximidad_en = Column(DateTime, nullable=True)
    alerta_incumplimiento_en = Column(DateTime, nullable=True)

    # Firma post-cuántica de los campos auditables (Ed25519 + ML-DSA-87, vía
    # Quipu). Ver app/services/signing_service.py.
    #
    # NULL es un estado legítimo y frecuente: la firma es OPCIONAL. Si
    # `quipu-crypto` no está instalado o no hay par de claves configurado, las
    # entregas se registran igual y esta columna queda vacía. Que la firma sea
    # un extra no puede convertirse en un motivo para no poder entregar.
    #
    # Lo que NO es negociable: si hay firma, tiene que corresponder a la entrega
    # tal como quedó guardada. Por eso se firma DENTRO de la misma transacción
    # que fija estado, fecha y archivo — no después, en una segunda pasada que
    # podría no llegar a ocurrir.
    firma = Column(Text, nullable=True)

    informe = relationship("Informe")
    usuario = relationship("Usuario", back_populates="entregas", foreign_keys=[usuario_id])
    creador_manual = relationship("Usuario", foreign_keys=[creador_manual_id])
    mensajes_chat = relationship("Comentario", back_populates="entrega", cascade="all, delete-orphan")

class Comentario(Base):
    __tablename__ = "comentarios_entrega"
    id = Column(Integer, primary_key=True, index=True)
    entrega_id = Column(Integer, ForeignKey("entregas_esperadas.id"))
    autor_id = Column(Integer, ForeignKey("usuarios.id"))
    texto = Column(String(1000), nullable=False)
    fecha = Column(DateTime, default=datetime.utcnow)
    entrega = relationship("EntregaEsperada", back_populates="mensajes_chat")
    autor = relationship("Usuario")

class PasswordReset(Base):
    __tablename__ = "password_resets"
    id = Column(Integer, primary_key=True, index=True)
    usuario_id = Column(Integer, ForeignKey("usuarios.id"))
    token = Column(String(100), unique=True, index=True)
    fecha_expiracion = Column(DateTime, nullable=False)
    usado = Column(Boolean, default=False)
    usuario = relationship("Usuario")

# =====================================================
# 7. AUDITORÍA DE SEGURIDAD (Ley 1581/2012)
# =====================================================

class AuditLog(Base):
    """
    Registro persistente de eventos de seguridad.

    Cumple con:
    - Ley 1581/2012 (Habeas Data): trazabilidad de accesos a datos personales
    - ISO 27001 A.12.4: registro y supervisión de eventos
    - Retención recomendada: mínimo 1 año (sector privado) o hasta 5 años
      según política interna de seguridad.
    """
    __tablename__ = "audit_logs"

    id = Column(Integer, primary_key=True, index=True)
    timestamp = Column(DateTime, default=datetime.utcnow, nullable=False, index=True)

    # Tipo de evento: login_attempt, logout, session_timeout, permission_denied
    evento = Column(String(50), nullable=False, index=True)

    # Identidad del actor
    email = Column(String(100), nullable=True, index=True)
    usuario_id = Column(Integer, ForeignKey("usuarios.id"), nullable=True)

    # Resultado del evento (aplica a login_attempt)
    exitoso = Column(Boolean, nullable=True)

    # Contexto de red
    ip_address = Column(String(45), nullable=True)  # IPv4 (15) o IPv6 (39)

    # Recurso o acción denegada (aplica a permission_denied)
    recurso = Column(String(200), nullable=True)

    # Detalles adicionales en JSON libre
    detalles = Column(JSON, nullable=True)

    usuario = relationship("Usuario")