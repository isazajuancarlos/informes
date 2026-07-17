FROM python:3.11-slim

WORKDIR /app

# Instalar dependencias del sistema
RUN apt-get update && apt-get install -y \
    postgresql-client \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copiar requirements
COPY informes/requirements.txt ./requirements.txt

# Instalar dependencias Python
RUN pip install --no-cache-dir --upgrade pip && \
    pip install --no-cache-dir -r requirements.txt

# Copiar código compartido
COPY core/shared /app/core/shared

# Copiar código del servicio
COPY informes /app/informes

# Variables de entorno
ENV PYTHONPATH=/app
ENV PYTHONUNBUFFERED=1

# Exponer puerto
EXPOSE 8003

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8003/health || exit 1

# Comando por defecto
CMD ["python", "informes/main.py"]
