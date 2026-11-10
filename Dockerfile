# One small image with the demo server and the built UI. Used by Render (render.yaml) and runnable anywhere:
#   docker build -t sthiranet-surveillance-demo . && docker run --rm -p 3000:3000 sthiranet-surveillance-demo

# ── UI: React + Vite build. The API types (src/api/schema.d.ts) are committed, so no Rust is needed here. ──
FROM node:22-bookworm-slim AS ui
WORKDIR /ui
COPY demo/ui/package.json demo/ui/package-lock.json ./
RUN npm ci
COPY demo/ui/ ./
RUN npm run build

# ── Server: surveillance-director, release build. ──
FROM rust:1-bookworm AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY demo/surveillance-director demo/surveillance-director
RUN cargo build --release --locked -p surveillance-director

# ── Runtime ──
FROM debian:bookworm-slim
RUN useradd --system --no-create-home demo
WORKDIR /app
COPY --from=server /src/target/release/surveillance-director /app/surveillance-director
COPY --from=ui /ui/dist /app/ui
ENV STHIRA_UI_DIST=/app/ui \
    PORT=3000
USER demo
EXPOSE 3000
CMD ["/app/surveillance-director"]
