# File Storage Setup with Docker Volume

## Changes Made

### 1. Docker Compose (`docker-compose.yml`)

- **Added volume mount** to the `api` service:
  ```yaml
  volumes:
    - osmium-files-data:/app/storage/files
  ```

- **Added `FILE_STORAGE_ROOT` environment variable** to the api service:
  ```yaml
  FILE_STORAGE_ROOT: /app/storage/files
  ```

- **Created named volume** `osmium-files-data` in the volumes section:
  ```yaml
  volumes:
    osmium-postgres-data:
    osmium-files-data:
  ```

### 2. Environment Configuration (`.env.example`)

Updated `FILE_STORAGE_ROOT` to:
```dotenv
# For Docker: /app/storage/files (uses osmium-files-data volume)
# For local development: ./storage/files
FILE_STORAGE_ROOT=/app/storage/files
```

## How It Works

- **Docker Volume**: Named volume `osmium-files-data` persists file uploads across container restarts
- **Container Path**: `/app/storage/files` inside the container is where files are stored
- **Host Storage**: The volume is managed by Docker (typically stored at `/var/lib/docker/volumes/`)
- **Persistence**: Files will remain even if the container is deleted and recreated

## Usage

### Start Docker with File Storage
```bash
docker-compose up --build
```

### View Stored Files (from host)
```bash
docker volume inspect osmium-files-data
```

### Access Files from Container
```bash
docker exec osmium-api ls -la /app/storage/files
```

### Backup Files
```bash
docker run --rm -v osmium-files-data:/data -v $(pwd):/backup \
  alpine tar czf /backup/files-backup.tar.gz -C /data .
```

### Clean Up Volume
```bash
# Remove the volume completely
docker volume rm osmium-files-data
```

## Local Development (Without Docker)

If you want to run locally without Docker, update `.env`:
```dotenv
FILE_STORAGE_ROOT=./storage/files
```

The application will create the directory automatically if it doesn't exist.

## Environment Variables

| Variable | Docker Value | Local Value | Description |
|----------|--------------|-------------|-------------|
| `FILE_STORAGE_ROOT` | `/app/storage/files` | `./storage/files` | Path where files are stored |
| `FILE_MAX_UPLOAD_BYTES` | `26214400` | `26214400` | Max file size (25 MB) |
| `FILE_SIGNING_SECRET` | Set in .env | Set in .env | Secret for signing download tokens |
| `CDN_BASE_URL` | `http://localhost:3000` | `http://localhost:3000` | Base URL for CDN links |

