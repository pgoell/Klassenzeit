# syntax=docker/dockerfile:1.9
#
# frontend/Dockerfile
#
# Build context: repo root. Two stages: node builder with pnpm via corepack,
# nginx:alpine runtime serving the static bundle on port 3000.

FROM node:22-alpine AS builder
WORKDIR /app/frontend

RUN corepack enable
RUN corepack prepare pnpm@10.33.2 --activate

# Leverage the layer cache: install deps from package.json + pnpm-lock.yaml
# alone, then copy the rest of the source tree.
COPY frontend/package.json frontend/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile

COPY frontend/ ./
RUN pnpm run build

FROM nginx:1.27-alpine AS runtime
COPY frontend/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/frontend/dist /usr/share/nginx/html

EXPOSE 3000
# nginx:alpine already has a healthy default CMD ["nginx", "-g", "daemon off;"];
# do not override.
