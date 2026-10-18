# syntax=docker/dockerfile:1
# Build context must be the repository root (see docker-compose.yml) so the
# client-generation step can read ../api/openapi.yaml.

FROM node:20-slim AS builder
WORKDIR /workspace/ui
COPY ui/package.json ui/package-lock.json ./
RUN npm ci
WORKDIR /workspace
COPY api/openapi.yaml ./api/openapi.yaml
COPY ui/ ./ui/
WORKDIR /workspace/ui
RUN npm run generate-client && npm run build

FROM nginx:1.27-alpine AS runtime
COPY ui/docker/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /workspace/ui/dist /usr/share/nginx/html
EXPOSE 80

HEALTHCHECK --interval=15s --timeout=3s --start-period=5s --retries=5 \
    CMD wget -q -O- http://127.0.0.1/ || exit 1
