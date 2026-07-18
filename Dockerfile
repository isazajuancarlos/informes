FROM python:3.11-slim

WORKDIR /app

# postgresql-client: pg_dump para los backups en perfiles cloud/onprem.
# Inofensivo en standalone (SQLite), donde el backup es copiar el fichero.
RUN apt-get update && apt-get install -y --no-install-recommends \
    postgresql-client \
    curl \
    && rm -rf /var/lib/apt/lists/*

COPY requirements.txt ./
RUN pip install --no-cache-dir --upgrade pip && \
    pip install --no-cache-dir -r requirements.txt

COPY . /app

# El perfil se elige en tiempo de ejecución (DEPLOYMENT_PROFILE); la imagen es
# la misma para cloud, onprem y standalone.
ENV PYTHONPATH=/app \
    PYTHONUNBUFFERED=1 \
    RELOAD=false

EXPOSE 8003

HEALTHCHECK --interval=30s --timeout=10s --start-period=20s --retries=3 \
    CMD curl -f http://localhost:8003/health || exit 1

CMD ["python", "main.py"]
